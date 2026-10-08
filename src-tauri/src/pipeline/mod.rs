//! The screenshot processing pipeline:
//! Photos → Vision OCR → local filter → DeepSeek (only if useful) → place resolution → merge → photos.
//! Every stage persists its status so an interrupted run resumes where it stopped.

pub mod actions;
pub mod merge;
pub mod queue;
pub mod reels;
pub mod regions;

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use anyhow::{anyhow, Result};

use crate::config::{AppConfig, AI_PROMPT_VERSION, OCR_VERSION};
use crate::db::{Database, OcrBlockRecord, ScreenshotRecord};
use crate::models::*;
use crate::services::ai::local_filter::{self, Route};
use crate::services::ai::types::*;
use crate::services::ai::TravelAIService;
use crate::services::native::{MediaService, OcrBlockData, OcrService, PhotoLibraryService, VisionService};
use crate::services::reels::ReelFetcher;
use crate::models::PlaceCandidate;
use crate::services::places::{Decision, PlaceError, PlaceService, ResolutionContext};
use merge::{attach, AttachRequest, Evidence, LineSource};

/// OCR blocks in reading order: top-to-bottom, then left-to-right on the same row.
pub fn reading_order(blocks: &[OcrBlockData]) -> Vec<OcrBlockData> {
    let mut v = blocks.to_vec();
    v.sort_by(|a, b| {
        let tolerance = a.height.min(b.height) * 0.5;
        let (ay, by) = (a.y + a.height / 2.0, b.y + b.height / 2.0);
        if (ay - by).abs() > tolerance { ay.total_cmp(&by) } else { a.x.total_cmp(&b.x) }
    });
    v
}

/// Injected dependencies — real bridge/DeepSeek in the app, mocks in tests.
pub struct Pipeline {
    pub db: Arc<Database>,
    pub photos: Arc<dyn PhotoLibraryService>,
    pub ocr: Arc<dyn OcrService>,
    pub vision: Arc<dyn VisionService>,
    pub places: Arc<PlaceService>,
    pub media: Arc<dyn MediaService>,
    pub fetcher: Arc<dyn ReelFetcher>,
    ai: RwLock<Arc<dyn TravelAIService>>,
    notifier: RwLock<Option<Arc<dyn Fn() + Send + Sync>>>,
    config: RwLock<AppConfig>,
    pub data_dir: PathBuf,
}

/// Outcome of a stage that could not finish for an external reason.
pub(crate) enum Stop {
    Waiting(String),
    Failed(String),
}

impl Pipeline {
    #[allow(clippy::too_many_arguments)]
    pub fn new(db: Arc<Database>, photos: Arc<dyn PhotoLibraryService>, ocr: Arc<dyn OcrService>, vision: Arc<dyn VisionService>,
               places: Arc<PlaceService>, media: Arc<dyn MediaService>, fetcher: Arc<dyn ReelFetcher>,
               ai: Arc<dyn TravelAIService>, config: AppConfig, data_dir: PathBuf) -> Self {
        Self {
            db, photos, ocr, vision, places, media, fetcher, ai: RwLock::new(ai), notifier: RwLock::new(None),
            config: RwLock::new(config), data_dir,
        }
    }

    /// Writes an orientation-corrected JPEG of a screenshot from Photos or from a folder.
    pub async fn export_source(&self, asset_id: &str, dest: &Path) -> Result<()> {
        match crate::services::folder::file_path(asset_id) {
            Some(file) => {
                anyhow::ensure!(file.exists(), "The file was moved or deleted: {}", file.display());
                self.photos.crop_image(&file, dest, Rect::new(0.0, 0.0, 1.0, 1.0), 2048).await
            }
            None => self.photos.export_image(asset_id, dest, 2048).await,
        }
    }

    /// Called whenever stored data changes outside the screenshot queue (e.g. Reel progress).
    pub fn set_notifier(&self, f: Arc<dyn Fn() + Send + Sync>) {
        *self.notifier.write().unwrap() = Some(f);
    }

    pub fn notify(&self) {
        if let Some(f) = self.notifier.read().unwrap().as_ref() {
            f();
        }
    }

    pub fn config(&self) -> AppConfig {
        self.config.read().unwrap().clone()
    }

    pub fn ai(&self) -> Arc<dyn TravelAIService> {
        self.ai.read().unwrap().clone()
    }

    pub fn reconfigure(&self, config: AppConfig, ai: Arc<dyn TravelAIService>) {
        *self.config.write().unwrap() = config;
        *self.ai.write().unwrap() = ai;
    }

    fn path(&self, folder: &str, name: &str) -> PathBuf {
        self.data_dir.join(folder).join(name)
    }

    fn time(&self, key: &str, start: Instant) {
        self.db.add_metric(&format!("{key}.ms"), start.elapsed().as_millis() as f64);
        self.db.add_metric(&format!("{key}.count"), 1.0);
    }

    /// Processes one screenshot end to end. Never panics or propagates errors for a single
    /// asset: failures are recorded on that screenshot only.
    pub async fn process(&self, screenshot_id: &str) -> ProcessingStatus {
        let start = Instant::now();
        let status = match self.run(screenshot_id).await {
            Ok(status) => status,
            Err(Stop::Waiting(detail)) => {
                let _ = self.db.set_status(screenshot_id, ProcessingStatus::WaitingForNetwork, Some(&detail));
                ProcessingStatus::WaitingForNetwork
            }
            Err(Stop::Failed(message)) => {
                log::warn!(target: "pipeline", "screenshot {screenshot_id} failed: {message}");
                let failures = self.db.record_failure(screenshot_id, &message).unwrap_or(1);
                if failures == 3 {
                    let _ = self.db.insert_review(ReviewKind::ProcessingFailure, Some(screenshot_id),
                        &format!("Processing failed 3 times: {message}"), None, &[], None, None, None);
                }
                self.db.add_metric("failures", 1.0);
                ProcessingStatus::Failed
            }
        };
        self.time("pipeline.total", start);
        let _ = self.db.set_pipeline_ms(screenshot_id, start.elapsed().as_millis() as i64);
        status
    }

    async fn run(&self, id: &str) -> Result<ProcessingStatus, Stop> {
        let fail = |e: anyhow::Error| Stop::Failed(e.to_string());
        let shot = self.db.screenshot(id).map_err(fail)?.ok_or_else(|| Stop::Failed("screenshot not found".into()))?;
        if shot.status == ProcessingStatus::Ignored {
            return Ok(ProcessingStatus::Ignored);
        }
        let config = self.config();

        // 1. Image + OCR (reused if already done — OCR is never repeated needlessly).
        let (image_path, blocks) = self.ensure_ocr(&shot).await?;
        let full_text = blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n");

        // 2. Level 1: local analysis.
        let report = local_filter::analyze(&full_text);
        self.db.set_local_analysis(id, report.score, report.source, report.creator.as_deref()).map_err(fail)?;
        let text_boxes: Vec<Rect> = blocks.iter().map(OcrBlockRecord::rect).collect();
        let rgb = load_rgb(&image_path).await;
        let local_regions = rgb.as_ref().map(|img| regions::detect(img, &text_boxes, &[], report.source)).unwrap_or_default();
        let has_large_photo = local_regions.iter().any(|r| r.region_type == RegionType::Photograph && r.rect.area() >= 0.25);

        let user = shot.user_classification();
        if user == Some(Classification::NotTravel) {
            self.db.set_classification(id, Classification::NotTravel, None, 1).map_err(fail)?;
            self.db.finish_screenshot(id, ProcessingStatus::NotTravel, Some("marked not travel by you")).map_err(fail)?;
            return Ok(ProcessingStatus::NotTravel);
        }
        let mut route = local_filter::route(&report, has_large_photo, &config);
        if user == Some(Classification::Travel) && route == Route::SkipNotTravel {
            route = if has_large_photo && config.allow_vision_requests { Route::AiWithImage } else { Route::AiText };
        }
        if route == Route::SkipNotTravel {
            self.db.set_classification(id, Classification::NotTravel, Some(report.score), 1).map_err(fail)?;
            self.db.finish_screenshot(id, ProcessingStatus::NotTravel, Some("skipped by local filter")).map_err(fail)?;
            self.db.add_metric("ai.avoidedByLocalFilter", 1.0);
            return Ok(ProcessingStatus::NotTravel);
        }

        // 3. Level 2 (and rarely 3): DeepSeek extraction.
        self.db.set_status(id, ProcessingStatus::Extracting, None).map_err(fail)?;
        let kept = local_filter::content_lines(&blocks.iter().map(|b| b.text.clone()).collect::<Vec<_>>());
        let mut input = ScreenshotAiInput {
            lines: kept.iter().map(|&i| blocks[i].text.clone()).collect(),
            line_block_ids: kept.iter().map(|&i| blocks[i].id.clone()).collect(),
            creation_date: shot.creation_date.clone(),
            source_hint: (report.source != SourceType::Unknown).then(|| report.source.as_str().to_string()),
            creator_hint: report.creator.clone(),
            image_jpeg: None,
        };
        if route == Route::AiWithImage {
            input.image_jpeg = rgb.as_ref().and_then(|img| compress_for_ai(img, config.vision_image_max_pixel_size));
        }
        let extraction = self.extract(&shot, &input, &config).await?;
        let p = extraction.travel_confidence.clamp(0.0, 1.0);

        let is_travel = user == Some(Classification::Travel)
            || (extraction.is_travel_related && p >= config.minimum_ai_confidence_for_auto_acceptance);
        if !is_travel {
            // Ask the user only when there is something to put on the map; otherwise it stays "low confidence"
            // (visible under that filter and overridable) instead of filling the Review inbox.
            if extraction.is_travel_related && p >= config.travel_review_threshold && !extraction.all_places().is_empty() {
                // 0.5–0.8: ask the user; the extraction is cached so "Yes" costs no new AI call.
                self.db.set_classification(id, Classification::Uncertain, Some(p), 2).map_err(fail)?;
                let msg = extraction.reason.clone().unwrap_or_else(|| "The AI isn't sure this is travel-related.".into());
                self.db.insert_review(ReviewKind::TravelClassification, Some(id), &msg, None, &[], None, None, None).map_err(fail)?;
                self.db.finish_screenshot(id, ProcessingStatus::NeedsReview, None).map_err(fail)?;
                return Ok(ProcessingStatus::NeedsReview);
            }
            let uncertain = extraction.is_travel_related && p >= config.travel_review_threshold;
            self.db.set_classification(id, if uncertain { Classification::Uncertain } else { Classification::NotTravel }, Some(p), 2).map_err(fail)?;
            let detail = if uncertain { Some("Possibly travel, but no specific place found".to_string()) } else { extraction.reason.clone() };
            self.db.finish_screenshot(id, ProcessingStatus::NotTravel, detail.as_deref()).map_err(fail)?;
            return Ok(ProcessingStatus::NotTravel);
        }
        self.db.set_classification(id, Classification::Travel, Some(p.max(if user.is_some() { 1.0 } else { 0.0 })), 2).map_err(fail)?;

        // 4–5. Resolve places, merge evidence, extract photos.
        let shot = self.db.screenshot(id).map_err(fail)?.unwrap_or(shot);
        self.resolve_and_attach(&shot, &extraction, &blocks, &input.line_block_ids, &image_path, rgb.as_ref(), &config).await
    }

    async fn ensure_ocr(&self, shot: &ScreenshotRecord) -> Result<(PathBuf, Vec<OcrBlockRecord>), Stop> {
        let fail = |e: anyhow::Error| Stop::Failed(e.to_string());
        let image_path = self.path("screenshots", &format!("{}.jpg", shot.id));
        let have_image = image_path.exists();
        if shot.status.can_reuse_ocr() && shot.ocr_version >= OCR_VERSION && have_image {
            return Ok((image_path, self.db.ocr_blocks(&shot.id).map_err(fail)?));
        }

        self.db.set_status(&shot.id, ProcessingStatus::Loading, None).map_err(fail)?;
        if !have_image {
            let t = Instant::now();
            // iCloud downloads fail transiently under load (real scan: 1,407 "offline" errors that all worked
            // moments later): retry with backoff, then park the screenshot for automatic retry.
            let mut attempt = 0u64;
            loop {
                match self.export_source(&shot.photos_id, &image_path).await {
                    Ok(()) => break,
                    Err(e) if is_transient_photos_error(&e.to_string()) => {
                        attempt += 1;
                        if attempt >= 3 {
                            return Err(Stop::Waiting(format!("iCloud Photos download will be retried: {e}")));
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(3 * attempt * attempt)).await;
                    }
                    Err(e) => return Err(fail(e)),
                }
            }
            self.time("photos.export", t);
        }
        let thumb = self.path("thumbnails", &format!("{}.jpg", shot.id));
        let thumb_ok = self.photos.crop_image(&image_path, &thumb, Rect::new(0.0, 0.0, 1.0, 1.0), 480).await.is_ok();

        if shot.ocr_version >= OCR_VERSION && shot.status.can_reuse_ocr() {
            self.db.set_image_paths(&shot.id, &image_path.to_string_lossy(), thumb_ok.then(|| thumb.to_string_lossy().to_string()).as_deref()).map_err(fail)?;
            return Ok((image_path, self.db.ocr_blocks(&shot.id).map_err(fail)?));
        }

        self.db.set_status(&shot.id, ProcessingStatus::OcrProcessing, None).map_err(fail)?;
        let t = Instant::now();
        let blocks = self.ocr.recognize(&image_path).await.map_err(|e| {
            self.db.add_metric("ocr.failures", 1.0);
            fail(e)
        })?;
        self.time("ocr", t);
        let _ = self.db.set_ocr_ms(&shot.id, t.elapsed().as_millis() as i64);
        let thumb_str = thumb_ok.then(|| thumb.to_string_lossy().to_string());
        let records = self.db.save_ocr(&shot.id, &blocks, &image_path.to_string_lossy(), thumb_str.as_deref()).map_err(fail)?;
        Ok((image_path, records))
    }

    /// Cached, budgeted, escalating AI extraction.
    async fn extract(&self, shot: &ScreenshotRecord, input: &ScreenshotAiInput, config: &AppConfig) -> Result<TravelExtraction, Stop> {
        let ai = self.ai();
        let model = ai.model_name();
        // Keyed on exactly what the AI would see (UI chrome, clock and battery already removed), so
        // near-identical screenshots share one result.
        let input_hash = crate::text::stable_hash(&input.lines.join("\n"));
        let cache_key = if input.image_jpeg.is_some() { format!("{input_hash}+image:{}", shot.id) } else { input_hash };

        // Never send the same content twice for the same model + prompt version.
        if let Ok(Some(json)) = self.db.ai_cache_get(&cache_key, &model, AI_PROMPT_VERSION, "extraction") {
            if let Ok(cached) = decode_json::<TravelExtraction>(&json) {
                self.db.add_metric("ai.cacheHits", 1.0);
                let _ = self.db.ai_cache_put(&format!("shot:{}", shot.id), &model, AI_PROMPT_VERSION, "extraction", &shot.id, &json);
                return Ok(cached);
            }
        }

        let mut result = self.call_extraction(shot, input, config.use_thinking_by_default, config).await?;
        let mut level = if config.use_thinking_by_default { 3 } else { 2 };

        // Level 3: one thinking retry, only for genuinely ambiguous results.
        let borderline = result.value.is_travel_related
            && result.value.travel_confidence >= config.travel_review_threshold
            && result.value.travel_confidence < config.minimum_ai_confidence_for_auto_acceptance;
        // Only worth it when there are places to disambiguate.
        let has_places = !result.value.all_places().is_empty();
        if level == 2 && config.allow_thinking_escalation && has_places && (result.value.is_ambiguous() || borderline) {
            if let Ok(better) = self.call_extraction(shot, input, true, config).await {
                result = better;
                level = 3;
            }
        }
        let _ = self.db.set_classification(&shot.id, Classification::Unknown, None, level);
        let _ = self.db.ai_cache_put(&cache_key, &model, AI_PROMPT_VERSION, "extraction", &shot.id, &result.raw_json);
        let _ = self.db.ai_cache_put(&format!("shot:{}", shot.id), &model, AI_PROMPT_VERSION, "extraction", &shot.id, &result.raw_json);
        Ok(result.value)
    }

    async fn call_extraction(&self, shot: &ScreenshotRecord, input: &ScreenshotAiInput, thinking: bool, config: &AppConfig) -> Result<AiResult<TravelExtraction>, Stop> {
        if config.daily_ai_request_limit > 0 && self.db.ai_requests_today().unwrap_or(0) >= config.daily_ai_request_limit as i64 {
            return Err(Stop::Waiting(AiError::DailyLimitReached.to_string()));
        }
        let t = Instant::now();
        let outcome = self.ai().extract_travel_information(input, thinking).await;
        self.time("ai.extract", t);
        match outcome {
            Ok(r) => {
                self.db.log_ai_usage(Some(&shot.id), "extraction", &r.usage, true, AI_PROMPT_VERSION);
                let source = r.value.source_type.as_deref().map(|s| SourceType::fuzzy(Some(s))).filter(|s| *s != SourceType::Unknown);
                let creator = r.value.creator.as_deref().map(str::trim).filter(|c| !c.is_empty());
                let _ = self.db.set_ai_details(&shot.id, source, creator, &r.usage);
                Ok(r)
            }
            Err(e) => {
                let usage = AiUsage { model: self.ai().model_name(), thinking, image_used: input.image_jpeg.is_some(), ..Default::default() };
                self.db.log_ai_usage(Some(&shot.id), "extraction", &usage, false, AI_PROMPT_VERSION);
                if e.is_transient() { Err(Stop::Waiting(e.to_string())) } else { Err(Stop::Failed(e.to_string())) }
            }
        }
    }

    /// Resolves every extracted place and attaches the evidence (screenshot or Reel).
    /// Returns the linked places with whether the AI flagged a useful photo for them.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn resolve_places(&self, evidence: Evidence<'_>, extraction: &TravelExtraction, blocks: &[OcrBlockRecord],
                                       line_sources: &[LineSource], user_names: &[String], config: &AppConfig) -> Result<Vec<(String, bool)>, Stop> {
        let fail = |e: anyhow::Error| Stop::Failed(e.to_string());
        let (screenshot_id, reel_id) = match evidence {
            Evidence::Screenshot(s) => (Some(s.id.as_str()), None),
            Evidence::Reel(r) => (None, Some(r.id.as_str())),
        };
        let review = |kind: ReviewKind, message: &str, place: &ExtractedPlace, candidates: &[PlaceCandidate], a: Option<&str>, b: Option<&str>| -> Result<()> {
            let id = self.db.insert_review(kind, screenshot_id, message, Some(place), candidates, a, b, None)?;
            if let Some(r) = reel_id {
                self.db.set_review_reel(&id, r)?;
            }
            Ok(())
        };

        let mut ctx = ResolutionContext::default();
        let mut linked: Vec<(String, bool)> = Vec::new();
        let mut auto_resolved = 0i64;
        let mut places = extraction.all_places();
        // Resolve the most specific places first so they anchor the rest ("7 places in Kyoto").
        places.sort_by_key(|p| p.ambiguous == Some(true));
        // Places the user already decided for this source are never re-resolved by AI, and names the user said
        // aren't a place here (removed pin, dismissed question) never come back.
        let rejected = self.db.rejected_source_names(screenshot_id, reel_id).map_err(fail)?;
        places.retain(|p| crate::text::best_similarity(&p.display_name, user_names) < 0.9);
        places.retain(|p| rejected.is_empty() || crate::text::best_similarity(&p.display_name, &rejected) < 0.9);
        // "Europe", "Japan"… are context, not places to pin.
        places.retain(|p| !crate::text::is_country_or_continent(&p.display_name));

        for place in &places {
            let t = Instant::now();
            let resolution = self.places.resolve(place, &ctx, config).await;
            self.time("maps.resolve", t);
            let resolution = match resolution {
                Ok(r) => r,
                Err(PlaceError::Throttled) => return Err(Stop::Waiting("Map search is throttled".into())),
                Err(e) => {
                    self.db.add_metric("maps.failures", 1.0);
                    return Err(Stop::Waiting(e.to_string()));
                }
            };
            let request = |cand, verification| AttachRequest {
                candidate: cand, extracted: Some(place), evidence, blocks, line_sources,
                confidence: resolution.confidence, verification, origin: DataOrigin::Ai,
            };
            match (resolution.decision, resolution.candidate.as_ref()) {
                (Decision::AutoAccept, Some(cand)) => {
                    let outcome = attach(&self.db, request(cand, Verification::Verified)).map_err(fail)?;
                    ctx.anchors.push((cand.latitude, cand.longitude));
                    self.db.add_metric(if outcome.created { "places.created" } else { "places.merged" }, 1.0);
                    self.db.add_metric("maps.autoResolved", 1.0);
                    auto_resolved += 1;
                    if let Some(other) = outcome.possible_duplicate_of {
                        review(ReviewKind::DuplicatePlace, "These may be the same place.", place, &[], Some(&outcome.place_id), Some(&other)).map_err(fail)?;
                    }
                    linked.push((outcome.place_id, place.has_useful_photo == Some(true)));
                }
                (Decision::Review, Some(cand)) => {
                    // Provisional place (hidden from the map) until the user confirms it.
                    let outcome = attach(&self.db, request(cand, Verification::NeedsReview)).map_err(fail)?;
                    review(ReviewKind::PlaceResolution, &format!("I found \"{}\" — {}", place.display_name, resolution.reason),
                           place, &resolution.alternatives, Some(&outcome.place_id), None).map_err(fail)?;
                    self.db.add_metric("maps.review", 1.0);
                    linked.push((outcome.place_id, place.has_useful_photo == Some(true)));
                }
                _ => {
                    review(ReviewKind::PlaceResolution,
                           &format!("I found \"{}\" but couldn't place it on the map — {}", place.display_name, resolution.reason),
                           place, &resolution.alternatives, None, None).map_err(fail)?;
                    self.db.add_metric("maps.unresolved", 1.0);
                }
            }
        }
        let (table, id) = match evidence {
            Evidence::Screenshot(s) => ("screenshots", s.id.as_str()),
            Evidence::Reel(r) => ("reels", r.id.as_str()),
        };
        let _ = self.db.set_place_counts(table, id, places.len() as i64, auto_resolved);
        Ok(linked)
    }

    /// Screenshot stage 4–5: resolve places, attach evidence, extract photos, set the final status.
    #[allow(clippy::too_many_arguments)]
    async fn resolve_and_attach(&self, shot: &ScreenshotRecord, extraction: &TravelExtraction, blocks: &[OcrBlockRecord],
                                line_block_ids: &[String], image_path: &Path, rgb: Option<&image::RgbImage>,
                                config: &AppConfig) -> Result<ProcessingStatus, Stop> {
        let fail = |e: anyhow::Error| Stop::Failed(e.to_string());
        self.db.set_status(&shot.id, ProcessingStatus::ResolvingPlaces, None).map_err(fail)?;
        let user_links: Vec<_> = self.db.links_for_screenshot(&shot.id).map_err(fail)?.into_iter().filter(|l| l.is_user_verified).collect();
        let user_names: Vec<String> = user_links.iter().map(|l| l.extracted_name.clone()).collect();
        let line_sources: Vec<LineSource> = line_block_ids.iter().map(|id| LineSource::block(id)).collect();

        let mut linked: Vec<(String, bool)> = user_links.iter().map(|l| (l.place_id.clone(), false)).collect();
        linked.extend(self.resolve_places(Evidence::Screenshot(shot), extraction, blocks, &line_sources, &user_names, config).await?);

        if !linked.is_empty() {
            self.db.set_status(&shot.id, ProcessingStatus::ExtractingImages, None).map_err(fail)?;
            if let Err(e) = self.extract_photos(shot, &linked, blocks, image_path, rgb, config).await {
                log::warn!(target: "images", "photo extraction failed for {}: {e}", shot.id); // never fails the screenshot
            }
        }

        let open = self.db.reviews_for_screenshot(&shot.id).map_err(fail)?.iter().any(|r| !r.is_resolved);
        let status = if open { ProcessingStatus::NeedsReview } else { ProcessingStatus::Complete };
        let detail = extraction.all_places().is_empty().then_some("no specific place found");
        self.db.finish_screenshot(&shot.id, status, detail).map_err(fail)?;
        Ok(status)
    }

    /// Local-first photo extraction. AI vision is consulted only for uncertain regions.
    async fn extract_photos(&self, shot: &ScreenshotRecord, linked: &[(String, bool)], blocks: &[OcrBlockRecord],
                            image_path: &Path, rgb: Option<&image::RgbImage>, config: &AppConfig) -> Result<()> {
        // Only attach a photo when it's clear which place it shows.
        let target = if linked.len() == 1 {
            Some(linked[0].0.clone())
        } else {
            let flagged: Vec<&(String, bool)> = linked.iter().filter(|(_, useful)| *useful).collect();
            (flagged.len() == 1).then(|| flagged[0].0.clone())
        };
        let (Some(place_id), Some(rgb)) = (target, rgb) else { return Ok(()) };

        let t = Instant::now();
        let saliency: Vec<Rect> = self.vision.saliency(image_path).await.map(|v| v.into_iter().map(|(r, _)| r).collect()).unwrap_or_default();
        let boxes: Vec<Rect> = blocks.iter().map(OcrBlockRecord::rect).collect();
        let candidates = regions::detect(rgb, &boxes, &saliency, shot.source_type);
        self.time("images.analyze", t);
        let Some(best) = candidates.into_iter().find(|c| c.region_type == RegionType::Photograph) else { return Ok(()) };
        if best.confidence < config.region_review_confidence {
            return Ok(());
        }

        let crop_path = self.path("crops", &format!("{}.jpg", crate::db::new_id()));
        self.photos.crop_image(image_path, &crop_path, best.rect, 1600).await?;

        let mut accepted = best.confidence >= config.region_auto_accept_confidence;
        let mut needs_review = !accepted;
        let mut confidence = best.confidence;
        if !accepted && config.allow_vision_requests {
            if let Some(small) = std::fs::read(&crop_path).ok().and_then(|d| image::load_from_memory(&d).ok())
                .and_then(|img| compress_for_ai(&img.to_rgb8(), 512))
            {
                if let Ok(r) = self.ai().classify_image_region(&small, "Region cropped from a travel screenshot").await {
                    self.db.log_ai_usage(Some(&shot.id), "region", &r.usage, true, AI_PROMPT_VERSION);
                    confidence = r.value.confidence;
                    let is_photo = r.value.kind() == RegionType::Photograph && r.value.is_useful_travel_photo != Some(false);
                    if is_photo && r.value.confidence >= 0.7 {
                        accepted = true;
                        needs_review = false;
                    } else if !is_photo && r.value.confidence >= 0.7 {
                        let _ = std::fs::remove_file(&crop_path);
                        return Ok(());
                    }
                }
            }
        }

        let print = self.vision.feature_print(image_path, Some(best.rect)).await.ok();
        if let Some(print) = &print {
            for existing in self.db.images_for_place(&place_id)? {
                let Some(other) = &existing.feature_print else { continue };
                if regions::feature_distance(print, other) < config.duplicate_image_distance {
                    self.db.add_metric("images.duplicates", 1.0);
                    if best.quality > existing.quality_score && accepted {
                        // Keep the better crop as primary, but keep provenance of the old one.
                        let id = self.db.insert_image(&place_id, Some(&shot.id), best.rect, &crop_path.to_string_lossy(), best.quality,
                                                      RegionType::Photograph, confidence, Some(print), true, DataOrigin::Local)?;
                        for sid in existing.screenshot_id.iter().chain(existing.duplicate_source_ids.iter()) {
                            self.db.add_duplicate_source(&id, sid)?;
                        }
                        if let Some(p) = &existing.image_path { let _ = std::fs::remove_file(p); }
                        self.db.delete_image(&existing.id)?;
                    } else {
                        self.db.add_duplicate_source(&existing.id, &shot.id)?;
                        let _ = std::fs::remove_file(&crop_path);
                    }
                    return Ok(());
                }
            }
        }

        let image_id = self.db.insert_image(&place_id, Some(&shot.id), best.rect, &crop_path.to_string_lossy(), best.quality,
                                            RegionType::Photograph, confidence, print.as_deref(), accepted, DataOrigin::Local)?;
        self.db.add_metric(if accepted { "images.accepted" } else { "images.review" }, 1.0);
        if needs_review {
            self.db.insert_review(ReviewKind::PhotoCrop, Some(&shot.id), "Use this image as a place photo?",
                                  None, &[], Some(&place_id), None, Some(&image_id))?;
        }
        Ok(())
    }

    /// Re-runs resolution from the cached extraction (e.g. after the user confirms "travel").
    pub async fn resolve_from_cache(&self, screenshot_id: &str) -> Result<ProcessingStatus> {
        let shot = self.db.screenshot(screenshot_id)?.ok_or_else(|| anyhow!("screenshot not found"))?;
        let Some(json) = self.db.cached_extraction_for_screenshot(screenshot_id)? else {
            // Nothing cached (e.g. skipped locally): run the full pipeline with the user's override.
            return Ok(self.process(screenshot_id).await);
        };
        let extraction: TravelExtraction = decode_json(&json)?;
        let blocks = self.db.ocr_blocks(screenshot_id)?;
        let kept = local_filter::content_lines(&blocks.iter().map(|b| b.text.clone()).collect::<Vec<_>>());
        let line_ids: Vec<String> = kept.iter().map(|&i| blocks[i].id.clone()).collect();
        let image_path = self.path("screenshots", &format!("{}.jpg", shot.id));
        let rgb = load_rgb(&image_path).await;
        self.db.set_classification(screenshot_id, Classification::Travel, None, 2)?;
        match self.resolve_and_attach(&shot, &extraction, &blocks, &line_ids, &image_path, rgb.as_ref(), &self.config()).await {
            Ok(s) => Ok(s),
            Err(Stop::Waiting(d)) => {
                self.db.set_status(screenshot_id, ProcessingStatus::WaitingForNetwork, Some(&d))?;
                Ok(ProcessingStatus::WaitingForNetwork)
            }
            Err(Stop::Failed(m)) => Err(anyhow!(m)),
        }
    }
}

async fn load_rgb(path: &Path) -> Option<image::RgbImage> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || image::open(path).ok().map(|i| i.to_rgb8())).await.ok().flatten()
}

/// Resized, compressed JPEG for AI vision requests (never the full-resolution screenshot).
pub fn compress_for_ai(img: &image::RgbImage, max_pixel: u32) -> Option<Vec<u8>> {
    let (w, h) = img.dimensions();
    let scale = (max_pixel as f64 / w.max(h) as f64).min(1.0);
    let resized = image::imageops::resize(img, ((w as f64 * scale) as u32).max(1), ((h as f64 * scale) as u32).max(1), image::imageops::FilterType::Triangle);
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 70).encode_image(&resized).ok()?;
    Some(out)
}


/// Photos/iCloud errors that go away on their own (network, iCloud download throttling).
pub fn is_transient_photos_error(message: &str) -> bool {
    let m = message.to_lowercase();
    ["offline", "network", "internet", "timed out", "icloud", "cloudphotolibrary", "try again"].iter().any(|k| m.contains(k))
}

#[cfg(test)]
mod transient_tests {
    #[test]
    fn real_icloud_errors_are_transient() {
        assert!(super::is_transient_photos_error("The internet connection appears to be offline."));
        assert!(!super::is_transient_photos_error("Screenshot no longer exists in Photos"));
    }
}
