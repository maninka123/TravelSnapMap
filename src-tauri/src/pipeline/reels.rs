//! Instagram Reel pipeline:
//! URL → metadata + video → audio transcript (on-device) → caption → keyframe OCR (Vision)
//! → ONE DeepSeek call → place resolution → merge into existing places (with timestamps as provenance).

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{anyhow, Result};

use super::merge::{attach, AttachRequest, Evidence, LineSource};
use super::{compress_for_ai, regions, Pipeline, Stop};
use crate::config::{AppConfig, AI_PROMPT_VERSION};
use crate::db::{ReelRecord, TranscriptSegment};
use crate::models::*;
use crate::services::ai::local_filter;
use crate::services::ai::types::*;
use crate::services::native::WordTiming;
use crate::services::reels::{is_instagram_url, shortcode};
use crate::text::normalize;

/// Groups word timings into readable, timestamped transcript lines.
pub fn group_words(words: &[WordTiming]) -> Vec<TranscriptSegment> {
    let mut out: Vec<TranscriptSegment> = Vec::new();
    let mut current: Option<TranscriptSegment> = None;
    let mut last_end = 0.0;
    for w in words {
        let text = w.text.trim();
        if text.is_empty() {
            continue;
        }
        let end = w.start + w.duration.max(0.0);
        let break_here = match &current {
            Some(seg) => w.start - last_end > 0.8 || seg.end - seg.start > 8.0 || seg.text.ends_with(['.', '!', '?']),
            None => false,
        };
        if break_here {
            out.extend(current.take());
        }
        match current.as_mut() {
            Some(seg) => {
                seg.text.push(' ');
                seg.text.push_str(text);
                seg.end = end;
            }
            None => current = Some(TranscriptSegment { start: w.start, end, text: text.to_string() }),
        }
        last_end = end;
    }
    out.extend(current);
    out
}

/// The numbered text evidence for one Reel, in priority order: audio, caption, on-screen text.
pub fn build_reel_lines(caption: Option<&str>, transcript: &[TranscriptSegment], frames: &[(f64, Vec<String>)]) -> (Vec<String>, Vec<LineSource>) {
    let mut lines = Vec::new();
    let mut sources = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut push = |text: String, key: String, source: LineSource, lines: &mut Vec<String>, sources: &mut Vec<LineSource>| {
        if seen.insert(key) {
            lines.push(text);
            sources.push(source);
        }
    };

    for seg in transcript {
        push(format!("audio {}: {}", mmss(seg.start), seg.text), normalize(&seg.text), LineSource { block_id: None, kind: "audio", time_sec: Some(seg.start) }, &mut lines, &mut sources);
    }
    if let Some(caption) = caption {
        for l in caption.lines().map(str::trim).filter(|l| l.chars().count() > 2).take(40) {
            push(format!("caption: {l}"), normalize(l), LineSource { block_id: None, kind: "caption", time_sec: None }, &mut lines, &mut sources);
        }
    }
    // On-screen text: only lines not already said or captioned, UI chrome removed, deduped across frames.
    for (time, texts) in frames {
        for i in local_filter::content_lines(texts) {
            let key = normalize(&texts[i]);
            if key.len() < 3 {
                continue;
            }
            push(format!("on-screen {}: {}", mmss(*time), texts[i]), key, LineSource { block_id: None, kind: "keyframe", time_sec: Some(*time) }, &mut lines, &mut sources);
        }
    }
    (lines, sources)
}

fn mmss(t: f64) -> String {
    let s = t.max(0.0) as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}

impl Pipeline {
    fn reel_dir(&self, id: &str) -> PathBuf {
        self.data_dir.join("reels").join(id)
    }

    /// Registers a pasted Instagram URL (same shortcode ⇒ same Reel). Processing is started by the caller.
    pub fn register_reel(&self, url: &str) -> Result<(String, bool)> {
        let url = url.trim();
        if !is_instagram_url(url) {
            return Err(anyhow!("That doesn't look like an Instagram Reel or post link."));
        }
        self.db.insert_reel(url, shortcode(url).as_deref())
    }

    /// Fallback: use a video from Photos (for a new Reel or an existing one without media).
    pub async fn attach_photos_video(&self, reel_id: Option<&str>, asset_id: &str) -> Result<String> {
        let id = match reel_id {
            Some(id) => id.to_string(),
            None => self.db.insert_reel(&format!("photos://{asset_id}"), None)?.0,
        };
        let out = self.reel_dir(&id).join("video.mp4");
        let created = self.media.export_video(asset_id, &out).await?;
        self.db.set_reel_media(&id, &out.to_string_lossy(), "photos", None)?;
        if let Some(date) = created {
            self.db.set_reel_metadata(&id, None, None, Some(&date), None)?;
        }
        self.db.clear_reel_derived(&id)?;
        Ok(id)
    }

    pub async fn reprocess_reel(&self, id: &str) -> ProcessingStatus {
        let _ = self.db.clear_reel_derived(id);
        self.process_reel(id).await
    }

    /// Processes one Reel end to end; a failure affects only this Reel.
    pub async fn process_reel(&self, id: &str) -> ProcessingStatus {
        let start = Instant::now();
        let status = match self.run_reel(id).await {
            Ok(s) => s,
            Err(Stop::Waiting(detail)) => {
                let _ = self.db.set_reel_status(id, ProcessingStatus::WaitingForNetwork, Some(&detail));
                ProcessingStatus::WaitingForNetwork
            }
            Err(Stop::Failed(message)) => {
                log::warn!(target: "reels", "reel {id} failed: {message}");
                let _ = self.db.record_reel_failure(id, &message);
                ProcessingStatus::Failed
            }
        };
        self.time("reels.total", start);
        self.notify();
        status
    }

    async fn run_reel(&self, id: &str) -> Result<ProcessingStatus, Stop> {
        let fail = |e: anyhow::Error| Stop::Failed(e.to_string());
        let config = self.config();
        let mut reel = self.db.reel(id).map_err(fail)?.ok_or_else(|| Stop::Failed("Reel not found".into()))?;
        let dir = self.reel_dir(id);

        // 1. Metadata + media (skipped when a video is already saved, e.g. imported from Photos).
        let has_media = reel.media_path.as_ref().is_some_and(|p| std::path::Path::new(p).exists());
        if !has_media && reel.url.starts_with("http") {
            self.db.set_reel_status(id, ProcessingStatus::Loading, Some("Fetching Reel")).map_err(fail)?;
            self.notify();
            let meta = match self.fetcher.metadata(&reel.url, &config).await {
                Ok(m) => m,
                Err(e) if e.to_string().to_lowercase().contains("connect") || e.to_string().contains("dns") => {
                    return Err(Stop::Waiting(format!("Offline: {e}")));
                }
                Err(e) => {
                    log::warn!(target: "reels", "metadata unavailable: {e}");
                    Default::default()
                }
            };
            self.db.set_reel_metadata(id, meta.creator.as_deref(), meta.caption.as_deref(), meta.posted_at.as_deref(), None).map_err(fail)?;
            let video = dir.join("video.mp4");
            if let Ok(Some(source)) = self.fetcher.download(&reel.url, &meta, &video, &config).await {
                self.db.set_reel_media(id, &video.to_string_lossy(), &source, meta.duration_sec).map_err(fail)?;
            } else if let Some(thumb) = &meta.thumbnail_url {
                let cover = dir.join("cover.jpg");
                if self.fetcher.download_file(thumb, &cover).await.is_ok() {
                    self.db.set_reel_metadata(id, None, None, None, Some(&cover.to_string_lossy())).map_err(fail)?;
                }
            }
            reel = self.db.reel(id).map_err(fail)?.unwrap_or(reel);
        }
        let media = reel.media_path.clone().filter(|p| std::path::Path::new(p).exists()).map(PathBuf::from);

        // 2. Audio first: save the voice track and transcribe it on-device with timestamps.
        let mut frames_text: Vec<(f64, Vec<String>)> = Vec::new();
        let mut transcript = reel.transcript.clone();
        let mut notes: Vec<String> = Vec::new();
        if let Some(video) = &media {
            self.db.set_reel_status(id, ProcessingStatus::OcrProcessing, Some("Transcribing audio")).map_err(fail)?;
            self.notify();
            let t = Instant::now();
            match self.media.extract_audio(video, &dir.join("audio.m4a")).await {
                Ok((Some(audio), duration)) => {
                    let _ = self.db.set_reel_media(id, &video.to_string_lossy(), reel.media_source.as_deref().unwrap_or("photos"), Some(duration));
                    match self.media.transcribe(&audio, &config.transcription_locale).await {
                        Ok(tr) => {
                            transcript = group_words(&tr.words);
                            let locale = format!("{}{}", tr.locale, if tr.on_device { "" } else { ", Apple server" });
                            self.db.set_reel_transcript(id, Some(&audio.to_string_lossy()), &transcript, Some(&locale)).map_err(fail)?;
                        }
                        Err(e) => {
                            notes.push(format!("transcription unavailable: {e}"));
                            self.db.set_reel_transcript(id, Some(&audio.to_string_lossy()), &[], None).map_err(fail)?;
                        }
                    }
                }
                Ok((None, duration)) => {
                    let _ = self.db.set_reel_media(id, &video.to_string_lossy(), reel.media_source.as_deref().unwrap_or("photos"), Some(duration));
                    notes.push("no audio track".into());
                }
                Err(e) => notes.push(format!("audio extraction failed: {e}")),
            }
            self.time("reels.transcribe", t);

            // 3. Secondary: OCR on a handful of visually distinct key frames (never every frame).
            self.db.set_reel_status(id, ProcessingStatus::OcrProcessing, Some("Reading key frames")).map_err(fail)?;
            self.notify();
            let t = Instant::now();
            let frames_dir = dir.join("frames");
            let _ = std::fs::remove_dir_all(&frames_dir);
            if let Ok((_, frames)) = self.media.keyframes(video, &frames_dir, config.max_keyframes).await {
                let mut rows = Vec::new();
                for f in frames {
                    let blocks = self.ocr.recognize(std::path::Path::new(&f.path)).await.unwrap_or_default();
                    let ordered = super::reading_order(&blocks);
                    let texts: Vec<String> = ordered.iter().map(|b| b.text.clone()).collect();
                    let blocks_json = serde_json::to_string(&ordered.iter().map(|b| serde_json::json!({
                        "text": b.text, "confidence": b.confidence, "x": b.x, "y": b.y, "width": b.width, "height": b.height
                    })).collect::<Vec<_>>()).unwrap_or_else(|_| "[]".into());
                    rows.push((f.time_sec, f.path.clone(), texts.join("\n"), blocks_json));
                    frames_text.push((f.time_sec, texts));
                }
                self.db.replace_keyframes(id, &rows).map_err(fail)?;
            }
            self.time("reels.keyframes", t);
        }
        reel = self.db.reel(id).map_err(fail)?.unwrap_or(reel);

        // 4. One DeepSeek call on text evidence (audio → caption → on-screen text).
        let (lines, line_sources) = build_reel_lines(reel.caption.as_deref(), &transcript, &frames_text);
        if lines.is_empty() {
            let detail = if media.is_none() {
                "Instagram didn't share this Reel's video or caption. Import the video from Photos to analyse it."
            } else {
                "No speech, caption or on-screen text found."
            };
            self.db.finish_reel(id, if media.is_none() { ProcessingStatus::NeedsMedia } else { ProcessingStatus::NotTravel }, Some(detail)).map_err(fail)?;
            return Ok(if media.is_none() { ProcessingStatus::NeedsMedia } else { ProcessingStatus::NotTravel });
        }
        self.db.set_reel_status(id, ProcessingStatus::Extracting, Some("Understanding the Reel")).map_err(fail)?;
        self.notify();
        let text_len: usize = lines.iter().map(|l| l.len()).sum();
        let cover = self.db.keyframes(id).ok().and_then(|k| k.into_iter().find(|f| f.is_cover));
        let image = if text_len < 60 && config.allow_vision_requests {
            // Only when the text evidence is too thin: one compressed cover frame, never the video.
            cover.as_ref().and_then(|c| image::open(&c.image_path).ok()).and_then(|img| compress_for_ai(&img.to_rgb8(), config.vision_image_max_pixel_size))
        } else {
            None
        };
        let input = ScreenshotAiInput {
            lines,
            line_block_ids: vec![],
            creation_date: reel.posted_at.clone(),
            source_hint: Some("instagram_reel".into()),
            creator_hint: reel.creator.clone(),
            image_jpeg: image,
        };
        let extraction = self.extract_reel(&reel, &input, &config).await?;

        // An imported Reel was saved on purpose: treat it as travel unless the AI clearly disagrees.
        let travel = extraction.is_travel_related || !extraction.all_places().is_empty();
        let classification = if travel { Classification::Travel } else { Classification::NotTravel };
        if !travel {
            self.db.finish_reel(id, ProcessingStatus::NotTravel, extraction.reason.as_deref()).map_err(fail)?;
            return Ok(ProcessingStatus::NotTravel);
        }
        let _ = classification;

        // 5. Resolve each place independently and merge with existing places.
        self.db.set_reel_status(id, ProcessingStatus::ResolvingPlaces, Some("Finding places")).map_err(fail)?;
        self.notify();
        let user_links = self.db.user_reel_links(id).map_err(fail)?;
        let user_names: Vec<String> = user_links.iter().map(|(_, n)| n.clone()).collect();
        let mut linked: Vec<(String, bool)> = user_links.iter().map(|(p, _)| (p.clone(), false)).collect();
        linked.extend(self.resolve_places(Evidence::Reel(&reel), &extraction, &[], &line_sources, &user_names, &config).await?);

        // 6. A clean key frame becomes a place photo when it's clear which place it shows.
        if let Err(e) = self.reel_photo(&reel, &linked, &config).await {
            log::warn!(target: "images", "reel photo extraction failed: {e}");
        }

        let open = self.db.reviews_for_reel(id).map_err(fail)?.iter().any(|r| !r.is_resolved);
        let (status, detail) = if open {
            (ProcessingStatus::NeedsReview, None)
        } else if media.is_none() && linked.is_empty() {
            (ProcessingStatus::NeedsMedia, Some("Only the caption was available. Import the video from Photos for more."))
        } else if media.is_none() {
            (ProcessingStatus::Complete, Some("From the caption only — import the video from Photos for more."))
        } else {
            (ProcessingStatus::Complete, if notes.is_empty() { None } else { Some(notes[0].as_str()) })
        };
        self.db.finish_reel(id, status, detail).map_err(fail)?;
        Ok(status)
    }

    async fn extract_reel(&self, reel: &ReelRecord, input: &ScreenshotAiInput, config: &AppConfig) -> Result<TravelExtraction, Stop> {
        let ai = self.ai();
        let model = ai.model_name();
        let hash = crate::text::stable_hash(&format!("reel\n{}", input.lines.join("\n")));
        if let Ok(Some(json)) = self.db.ai_cache_get(&hash, &model, AI_PROMPT_VERSION, "reel") {
            if let Ok(cached) = decode_json::<TravelExtraction>(&json) {
                self.db.add_metric("ai.cacheHits", 1.0);
                return Ok(cached);
            }
        }
        if config.daily_ai_request_limit > 0 && self.db.ai_requests_today().unwrap_or(0) >= config.daily_ai_request_limit as i64 {
            return Err(Stop::Waiting(AiError::DailyLimitReached.to_string()));
        }
        let t = Instant::now();
        let mut outcome = ai.extract_travel_information(input, config.use_thinking_by_default).await;
        // Level 3 only for genuinely ambiguous results.
        if let Ok(r) = &outcome {
            if !config.use_thinking_by_default && config.allow_thinking_escalation && r.value.is_ambiguous() {
                self.db.log_ai_usage(None, "reel", &r.usage, true, AI_PROMPT_VERSION);
                if let Ok(better) = ai.extract_travel_information(input, true).await {
                    outcome = Ok(better);
                }
            }
        }
        self.time("ai.reel", t);
        match outcome {
            Ok(r) => {
                self.db.log_ai_usage(None, "reel", &r.usage, true, AI_PROMPT_VERSION);
                let cls = if r.value.is_travel_related { Classification::Travel } else { Classification::NotTravel };
                let _ = self.db.set_reel_ai(&reel.id, cls, r.value.travel_confidence, &hash, &model, AI_PROMPT_VERSION,
                                            r.usage.input_tokens, r.usage.output_tokens, r.usage.estimated_cost);
                if reel.creator.is_none() {
                    let _ = self.db.set_reel_metadata(&reel.id, r.value.creator.as_deref(), None, None, None);
                }
                let _ = self.db.ai_cache_put(&hash, &model, AI_PROMPT_VERSION, "reel", &reel.id, &r.raw_json);
                Ok(r.value)
            }
            Err(e) => {
                let usage = AiUsage { model, image_used: input.image_jpeg.is_some(), ..Default::default() };
                self.db.log_ai_usage(None, "reel", &usage, false, AI_PROMPT_VERSION);
                if e.is_transient() { Err(Stop::Waiting(e.to_string())) } else { Err(Stop::Failed(e.to_string())) }
            }
        }
    }

    async fn reel_photo(&self, reel: &ReelRecord, linked: &[(String, bool)], config: &AppConfig) -> Result<()> {
        let target = if linked.len() == 1 {
            Some(linked[0].0.clone())
        } else {
            let flagged: Vec<_> = linked.iter().filter(|(_, useful)| *useful).collect();
            (flagged.len() == 1).then(|| flagged[0].0.clone())
        };
        let Some(place_id) = target else { return Ok(()) };
        let Some(cover) = self.db.keyframes(&reel.id)?.into_iter().find(|k| k.is_cover) else { return Ok(()) };
        let path = PathBuf::from(&cover.image_path);
        let Ok(img) = image::open(&path) else { return Ok(()) };
        let text_boxes: Vec<Rect> = vec![]; // cover frame is the one with the least on-screen text
        let regions = regions::detect(&img.to_rgb8(), &text_boxes, &[], SourceType::Instagram);
        let Some(best) = regions.into_iter().find(|r| r.region_type == RegionType::Photograph && r.confidence >= config.region_auto_accept_confidence) else {
            return Ok(());
        };
        let crop = self.data_dir.join("crops").join(format!("{}.jpg", crate::db::new_id()));
        self.photos.crop_image(&path, &crop, best.rect, 1600).await?;
        let print = self.vision.feature_print(&path, Some(best.rect)).await.ok();
        if let Some(print) = &print {
            for existing in self.db.images_for_place(&place_id)? {
                if existing.feature_print.as_ref().is_some_and(|o| regions::feature_distance(print, o) < config.duplicate_image_distance) {
                    let _ = std::fs::remove_file(&crop);
                    return Ok(());
                }
            }
        }
        let image_id = self.db.insert_image(&place_id, None, best.rect, &crop.to_string_lossy(), best.quality, RegionType::Photograph,
                                            best.confidence, print.as_deref(), true, DataOrigin::Local)?;
        self.db.set_image_reel(&image_id, &reel.id)?;
        Ok(())
    }

    /// Links a Reel to a place the user picked.
    pub fn user_attach_reel(&self, reel_id: &str, candidate: &PlaceCandidate, extracted: Option<&ExtractedPlace>) -> Result<String> {
        let reel = self.db.reel(reel_id)?.ok_or_else(|| anyhow!("Reel not found"))?;
        let outcome = attach(&self.db, AttachRequest {
            candidate, extracted, evidence: Evidence::Reel(&reel), blocks: &[], line_sources: &[], confidence: 1.0,
            verification: Verification::UserVerified, origin: DataOrigin::User,
        })?;
        self.db.set_place_user_verified(&outcome.place_id)?;
        Ok(outcome.place_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(text: &str, start: f64, duration: f64) -> WordTiming {
        WordTiming { text: text.into(), start, duration }
    }

    #[test]
    fn groups_words_into_timestamped_sentences() {
        let words = vec![
            w("This", 3.0, 0.2), w("is", 3.2, 0.2), w("Lake", 3.4, 0.3), w("Kawaguchi.", 3.7, 0.5),
            w("Come", 7.0, 0.2), w("early", 7.2, 0.3), w("for", 7.5, 0.1), w("clearer", 7.6, 0.3), w("views", 7.9, 0.3),
            w("Take", 13.0, 0.2), w("the", 13.2, 0.1), w("train", 13.3, 0.3),
        ];
        let segs = group_words(&words);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].text, "This is Lake Kawaguchi.");
        assert_eq!(segs[0].start, 3.0);
        assert_eq!(segs[1].text, "Come early for clearer views");
        assert_eq!(segs[2].start, 13.0);
    }

    #[test]
    fn reel_lines_keep_provenance_and_skip_repeats() {
        let transcript = vec![TranscriptSegment { start: 7.0, end: 9.0, text: "Come early for clearer Fuji views".into() }];
        let frames = vec![
            (5.0, vec!["Lake Kawaguchi".to_string(), "Follow".to_string()]),
            (9.0, vec!["Lake Kawaguchi".to_string(), "¥500 entry".to_string()]),
        ];
        let (lines, sources) = build_reel_lines(Some("Japan trip 🗻\nLake Kawaguchi"), &transcript, &frames);
        assert_eq!(lines[0], "audio 00:07: Come early for clearer Fuji views");
        assert_eq!(sources[0].kind, "audio");
        assert_eq!(sources[0].time_sec, Some(7.0));
        assert!(lines.contains(&"caption: Lake Kawaguchi".to_string()));
        // "Lake Kawaguchi" on screen was already in the caption; "Follow" is UI chrome.
        let on_screen: Vec<&String> = lines.iter().filter(|l| l.starts_with("on-screen")).collect();
        assert_eq!(on_screen, vec!["on-screen 00:09: ¥500 entry"]);
        assert_eq!(sources[lines.len() - 1].time_sec, Some(9.0));
    }
}
