//! End-to-end pipeline tests with mock Photos, OCR, Vision, map provider and AI.
//! No live APIs are used.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;

use crate::config::AppConfig;
use crate::db::{Database, PlaceFilter};
use crate::models::*;
use crate::pipeline::{merge, Pipeline};
use crate::services::ai::types::*;
use crate::services::ai::TravelAIService;
use crate::services::native::*;
use crate::services::places::{PlaceError, PlaceSearchProvider, PlaceService};
use crate::services::reels::{ReelFetcher, ReelMetadata};

// MARK: Mocks

struct MockPhotos;

#[async_trait]
impl PhotoLibraryService for MockPhotos {
    async fn authorization_status(&self) -> Result<String> { Ok("authorized".into()) }
    async fn request_permission(&self) -> Result<String> { Ok("authorized".into()) }
    async fn list_screenshots(&self, _: Option<i32>, _: Option<i32>) -> Result<Vec<AssetInfo>> { Ok(vec![]) }
    async fn metadata(&self, _: &str) -> Result<Value> { Ok(Value::Null) }
    async fn export_image(&self, _: &str, path: &Path, _: u32) -> Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        image::RgbImage::from_pixel(120, 260, image::Rgb([250, 250, 250])).save(path)?;
        Ok(())
    }
    async fn crop_image(&self, source: &Path, dest: &Path, _: Rect, _: u32) -> Result<()> {
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::copy(source, dest)?;
        Ok(())
    }
    async fn observe_new_screenshots(&self) -> Result<()> { Ok(()) }
}

/// OCR text per photos asset id (the file name carries the screenshot id, so we map via the DB).
struct MockOcr {
    texts: Mutex<HashMap<String, String>>,
    calls: AtomicUsize,
}

#[async_trait]
impl OcrService for MockOcr {
    async fn recognize(&self, image: &Path) -> Result<Vec<OcrBlockData>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let key = image.file_stem().unwrap().to_string_lossy().to_string();
        let text = self.texts.lock().unwrap().get(&key).cloned().unwrap_or_default();
        Ok(text.lines().enumerate().map(|(i, l)| OcrBlockData {
            text: l.into(), confidence: 0.99, x: 0.05, y: 0.05 + i as f64 * 0.05, width: 0.8, height: 0.03, language: None,
        }).collect())
    }
}

struct MockVision;

#[async_trait]
impl VisionService for MockVision {
    async fn saliency(&self, _: &Path) -> Result<Vec<(Rect, f64)>> { Ok(vec![]) }
    async fn feature_print(&self, _: &Path, _: Option<Rect>) -> Result<Vec<f32>> { Ok(vec![1.0, 0.0]) }
}

struct MockMaps {
    results: HashMap<String, Vec<PlaceCandidate>>,
}

#[async_trait]
impl PlaceSearchProvider for MockMaps {
    async fn search(&self, query: &str, _near: Option<&str>) -> Result<Vec<PlaceCandidate>, PlaceError> {
        let q = crate::text::normalize(query);
        Ok(self.results.iter().filter(|(k, _)| q.starts_with(&crate::text::normalize(k))).flat_map(|(_, v)| v.clone()).collect())
    }
}

enum AiBehaviour {
    Json(String),
    Offline,
}

struct MockAi {
    /// First OCR line → response.
    responses: Mutex<HashMap<String, AiBehaviour>>,
    calls: AtomicUsize,
}

#[async_trait]
impl TravelAIService for MockAi {
    async fn classify_screenshot(&self, _: &ScreenshotAiInput, _: bool) -> Result<AiResult<TravelClassification>, AiError> {
        Err(AiError::InvalidResponse)
    }
    async fn extract_travel_information(&self, input: &ScreenshotAiInput, thinking: bool) -> Result<AiResult<TravelExtraction>, AiError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let key = input.lines.first().cloned().unwrap_or_default();
        let responses = self.responses.lock().unwrap();
        match responses.get(&key) {
            Some(AiBehaviour::Json(json)) => Ok(AiResult {
                value: decode_json(json)?,
                usage: AiUsage { model: "mock".into(), thinking, ..Default::default() },
                raw_json: json.clone(),
            }),
            Some(AiBehaviour::Offline) => Err(AiError::Offline),
            None => Err(AiError::InvalidJson("no mock response".into())),
        }
    }
    async fn classify_image_region(&self, _: &[u8], _: &str) -> Result<AiResult<RegionClassification>, AiError> {
        Err(AiError::InvalidResponse)
    }
    async fn summarize_place(&self, _: &str, facts: &[(String, String)]) -> Result<AiResult<PlaceSummary>, AiError> {
        Ok(AiResult {
            value: PlaceSummary { summary: "You saved this place for sunrise views.".into(), used_fact_ids: Some(vec![facts[0].0.clone()]) },
            usage: AiUsage::default(), raw_json: String::new(),
        })
    }
    fn model_name(&self) -> String { "mock".into() }
}

// MARK: Harness

fn cand(name: &str, lat: f64, lon: f64, code: &str, country: &str, city: &str, map_id: &str) -> PlaceCandidate {
    PlaceCandidate {
        name: name.into(), latitude: lat, longitude: lon, country: Some(country.into()), country_code: Some(code.into()),
        city: Some(city.into()), map_identifier: Some(map_id.into()), ..Default::default()
    }
}

struct Harness {
    pipeline: Arc<Pipeline>,
    db: Arc<Database>,
    ocr: Arc<MockOcr>,
    ai: Arc<MockAi>,
    media: Arc<MockMedia>,
    fetcher: Arc<MockFetcher>,
    _dir: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let mut maps = HashMap::new();
        maps.insert("Fushimi Inari".to_string(), vec![cand("Fushimi Inari Taisha", 34.9671, 135.7727, "JP", "Japan", "Kyoto", "m-fushimi")]);
        maps.insert("Kiyomizu-dera".to_string(), vec![cand("Kiyomizu-dera", 34.9949, 135.7850, "JP", "Japan", "Kyoto", "m-kiyomizu")]);
        maps.insert("Nishiki Market".to_string(), vec![cand("Nishiki Market", 35.0050, 135.7649, "JP", "Japan", "Kyoto", "m-nishiki")]);
        maps.insert("Arashiyama Bamboo Grove".to_string(), vec![cand("Arashiyama Bamboo Grove", 35.0170, 135.6713, "JP", "Japan", "Kyoto", "m-arashiyama")]);
        maps.insert("Tokyo Sky Tree".to_string(), vec![cand("Tokyo Skytree", 35.7101, 139.8107, "JP", "Japan", "Tokyo", "m-skytree")]);
        maps.insert("Tokyo Skytree".to_string(), vec![cand("Tokyo Skytree", 35.7101, 139.8107, "JP", "Japan", "Tokyo", "m-skytree")]);
        maps.insert("Lake Kawaguchi".to_string(), vec![cand("Lake Kawaguchi", 35.5173, 138.7561, "JP", "Japan", "Fujikawaguchiko", "m-kawaguchi")]);
        maps.insert("Chureito Pagoda".to_string(), vec![cand("Chureito Pagoda", 35.5011, 138.8013, "JP", "Japan", "Fujiyoshida", "m-chureito")]);
        maps.insert("Blue Lagoon".to_string(), vec![
            cand("Blue Lagoon", 35.95, 14.33, "MT", "Malta", "Comino", "m-bl-malta"),
            cand("Blue Lagoon", 63.88, -22.45, "IS", "Iceland", "Grindavík", "m-bl-iceland"),
        ]);

        let db = Arc::new(Database::open_in_memory().unwrap());
        let dir = std::env::temp_dir().join(format!("tsm-test-{}", uuid::Uuid::new_v4()));
        let ocr = Arc::new(MockOcr { texts: Mutex::new(HashMap::new()), calls: AtomicUsize::new(0) });
        let ai = Arc::new(MockAi { responses: Mutex::new(HashMap::new()), calls: AtomicUsize::new(0) });
        let places = Arc::new(PlaceService::new(Arc::new(MockMaps { results: maps })).with_min_interval(std::time::Duration::ZERO));
        let media = Arc::new(MockMedia { words: Mutex::new(vec![]), frames: Mutex::new(vec![]) });
        let fetcher = Arc::new(MockFetcher { caption: Mutex::new(None), has_video: AtomicUsize::new(1) });
        let pipeline = Arc::new(Pipeline::new(
            db.clone(), Arc::new(MockPhotos), ocr.clone(), Arc::new(MockVision), places, media.clone(), fetcher.clone(),
            ai.clone(), AppConfig::default(), dir.clone(),
        ));
        Self { pipeline, db, ocr, ai, media, fetcher, _dir: dir }
    }

    /// Adds a screenshot whose OCR returns `text`, with an AI response keyed on its first line.
    fn screenshot(&self, photos_id: &str, date: &str, text: &str, ai: Option<AiBehaviour>) -> String {
        self.db.upsert_discovered(&[AssetInfo { id: photos_id.into(), creation_date: Some(date.into()), width: 1170, height: 2532 }]).unwrap();
        let id: String = self.db.with(|c| c.query_row("SELECT id FROM screenshots WHERE photos_id = ?1", [photos_id], |r| r.get(0))).unwrap();
        self.ocr.texts.lock().unwrap().insert(id.clone(), text.into());
        if let Some(b) = ai {
            self.ai.responses.lock().unwrap().insert(text.lines().next().unwrap().to_string(), b);
        }
        id
    }

    fn places(&self) -> Vec<crate::db::PlaceRecord> {
        self.db.list_places(&PlaceFilter::default()).unwrap()
    }
}

fn json(s: &str) -> Option<AiBehaviour> {
    Some(AiBehaviour::Json(s.to_string()))
}

const KYOTO: &str = r#"{"is_travel_related": true, "travel_confidence": 0.97, "source_type": "instagram", "creator": "@kyotoguide",
  "places": [
    {"display_name": "Fushimi Inari", "city": "Kyoto", "country": "Japan", "category": "temple",
     "facts": [{"type": "recommended_time", "text": "Go before 8 AM to avoid crowds", "source_lines": [2]}]},
    {"display_name": "Kiyomizu-dera", "city": "Kyoto", "country": "Japan", "category": "temple"},
    {"display_name": "Nishiki Market", "city": "Kyoto", "country": "Japan", "category": "food"},
    {"display_name": "Arashiyama Bamboo Grove", "city": "Kyoto", "country": "Japan", "category": "nature"}
  ]}"#;

// MARK: Tests

#[tokio::test]
async fn multiple_places_from_one_screenshot() {
    let h = Harness::new();
    let id = h.screenshot("a1", "2025-03-01T00:00:00Z",
        "5 places you need to visit in Kyoto\n1. Fushimi Inari\nGo before 8 AM to avoid crowds\n2. Kiyomizu-dera\n3. Nishiki Market\n4. Arashiyama Bamboo Grove",
        json(KYOTO));
    assert_eq!(h.pipeline.process(&id).await, ProcessingStatus::Complete);

    let places = h.places();
    assert_eq!(places.len(), 4);
    assert!(places.iter().all(|p| p.verification == Verification::Verified && p.source_count == 1));
    assert_eq!(h.db.links_for_screenshot(&id).unwrap().len(), 4);

    let fushimi = places.iter().find(|p| p.canonical_name == "Fushimi Inari Taisha").unwrap();
    assert_eq!(fushimi.category, PlaceCategory::Temple);
    let facts = h.db.facts_for_place(&fushimi.id).unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].fact_type, TravelFactType::RecommendedTime);
    assert_eq!(facts[0].screenshot_id.as_deref(), Some(id.as_str()));
    // Provenance: the fact points at the exact OCR block it came from.
    let block = h.db.ocr_blocks(&id).unwrap().into_iter().find(|b| b.text.starts_with("Go before")).unwrap();
    assert_eq!(facts[0].source_block_ids, vec![block.id]);

    let shot = h.db.screenshot(&id).unwrap().unwrap();
    assert_eq!(shot.creator.as_deref(), Some("@kyotoguide"));
    assert_eq!(shot.source_type, SourceType::Instagram);
    assert_eq!(shot.escalation_level, 2);
}

#[tokio::test]
async fn several_screenshots_merge_into_one_place_and_keep_contradictions() {
    let h = Harness::new();
    let a = h.screenshot("s1", "2024-05-01T00:00:00Z", "Tokyo Skytree tips\nBest at sunrise\nTickets ¥2,000",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Tokyo Skytree", "city": "Tokyo", "country": "Japan",
          "facts": [{"type": "recommended_time", "text": "Best at sunrise"}, {"type": "price", "text": "Tickets ¥2,000"}]}]}"#));
    let b = h.screenshot("s2", "2026-02-01T00:00:00Z", "Tokyo Sky Tree view\nBest at sunset\nTickets ¥2,500\nBest at sunrise",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.93, "places": [{"display_name": "Tokyo Sky Tree", "alternative_names": ["東京スカイツリー"], "city": "Tokyo", "country": "Japan",
          "facts": [{"type": "recommended_time", "text": "Best at sunset"}, {"type": "price", "text": "Tickets ¥2,500"}, {"type": "recommended_time", "text": "Best at sunrise"}]}]}"#));
    h.pipeline.process(&a).await;
    h.pipeline.process(&b).await;

    let places = h.places();
    assert_eq!(places.len(), 1, "same map entity must not be duplicated");
    let p = &places[0];
    assert_eq!(p.source_count, 2);
    assert!(p.alternative_names.contains(&"東京スカイツリー".to_string()));

    let facts = h.db.facts_for_place(&p.id).unwrap();
    let texts: Vec<&str> = facts.iter().map(|f| f.text.as_str()).collect();
    // Contradictory advice and changed prices are both kept; the exact duplicate is not.
    assert_eq!(facts.len(), 4, "{texts:?}");
    assert!(texts.contains(&"Best at sunrise") && texts.contains(&"Best at sunset"));
    assert!(texts.contains(&"Tickets ¥2,000") && texts.contains(&"Tickets ¥2,500"));
    let new_price = facts.iter().find(|f| f.text == "Tickets ¥2,500").unwrap();
    assert_eq!(new_price.valid_from.as_deref(), Some("2026-02-01T00:00:00Z"));
}

#[tokio::test]
async fn clearly_unrelated_screenshots_never_reach_the_ai() {
    let h = Harness::new();
    let id = h.screenshot("code", "2025-01-01T00:00:00Z", "import SwiftUI\nfunc body() {}\nerror: build failed in Xcode", None);
    assert_eq!(h.pipeline.process(&id).await, ProcessingStatus::NotTravel);
    assert_eq!(h.ai.calls.load(Ordering::SeqCst), 0);
    assert_eq!(h.db.screenshot(&id).unwrap().unwrap().escalation_level, 1);
}

#[tokio::test]
async fn identical_ocr_is_served_from_the_ai_cache() {
    let h = Harness::new();
    let text = "Tokyo Skytree\nBest at sunrise\nThings to do in Japan";
    let resp = r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Tokyo Skytree", "country": "Japan"}]}"#;
    let a = h.screenshot("c1", "2025-01-01T00:00:00Z", text, json(resp));
    let b = h.screenshot("c2", "2025-01-02T00:00:00Z", text, None);
    h.pipeline.process(&a).await;
    h.pipeline.process(&b).await;
    assert_eq!(h.ai.calls.load(Ordering::SeqCst), 1);
    assert_eq!(h.places()[0].source_count, 2);
}

#[tokio::test]
async fn ambiguous_place_goes_to_review_and_user_choice_wins() {
    let h = Harness::new();
    let id = h.screenshot("bl", "2025-01-01T00:00:00Z", "Blue Lagoon\nMust visit! Travel goals",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.9, "places": [{"display_name": "Blue Lagoon", "ambiguous": true}]}"#));
    assert_eq!(h.pipeline.process(&id).await, ProcessingStatus::NeedsReview);
    // No verified pin was created for an uncertain match.
    let verified = h.db.list_places(&PlaceFilter { verified_only: true, ..Default::default() }).unwrap();
    assert!(verified.is_empty());

    let review = h.db.open_reviews().unwrap().into_iter().find(|r| r.kind == ReviewKind::PlaceResolution).unwrap();
    assert!(review.candidates.len() >= 2);
    let iceland = review.candidates.iter().find(|c| c.country_code.as_deref() == Some("IS")).unwrap().clone();
    h.pipeline.resolve_review(&review.id, "choose", Some(iceland)).await.unwrap();

    let places = h.places();
    assert_eq!(places.len(), 1);
    assert_eq!(places[0].country_code.as_deref(), Some("IS"));
    assert!(places[0].is_user_verified);
    assert_eq!(h.db.screenshot(&id).unwrap().unwrap().status, ProcessingStatus::Complete);

    // Re-processing never overrides the user's decision.
    h.pipeline.reprocess(&id).await.unwrap();
    let places = h.places();
    assert_eq!(places.len(), 1, "{:?}", places.iter().map(|p| &p.country).collect::<Vec<_>>());
    assert_eq!(places[0].country_code.as_deref(), Some("IS"));
}

#[tokio::test]
async fn malformed_ai_output_fails_only_that_screenshot() {
    let h = Harness::new();
    let bad = h.screenshot("bad", "2025-01-01T00:00:00Z", "Hotel in Paris\nper night booking", Some(AiBehaviour::Json("{oops".into())));
    let good = h.screenshot("good", "2025-01-01T00:00:00Z", "Tokyo Skytree\nThings to do in Japan",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Tokyo Skytree", "country": "Japan"}]}"#));
    assert_eq!(h.pipeline.process(&bad).await, ProcessingStatus::Failed);
    assert_eq!(h.pipeline.process(&good).await, ProcessingStatus::Complete);
    assert_eq!(h.places().len(), 1);
    assert!(h.db.links_for_screenshot(&bad).unwrap().is_empty());
}

#[tokio::test]
async fn offline_parks_the_screenshot_and_reuses_ocr_later() {
    let h = Harness::new();
    let text = "Tokyo Skytree\nThings to do in Japan";
    let id = h.screenshot("off", "2025-01-01T00:00:00Z", text, Some(AiBehaviour::Offline));
    assert_eq!(h.pipeline.process(&id).await, ProcessingStatus::WaitingForNetwork);
    assert_eq!(h.ocr.calls.load(Ordering::SeqCst), 1);

    h.ai.responses.lock().unwrap().insert("Tokyo Skytree".into(), AiBehaviour::Json(
        r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Tokyo Skytree", "country": "Japan"}]}"#.into()));
    assert_eq!(h.pipeline.process(&id).await, ProcessingStatus::Complete);
    assert_eq!(h.ocr.calls.load(Ordering::SeqCst), 1, "OCR must not run twice");
}

#[tokio::test]
async fn uncertain_travel_asks_then_uses_cached_extraction() {
    let h = Harness::new();
    let id = h.screenshot("u", "2025-01-01T00:00:00Z", "Tokyo Skytree\nmaybe visit someday? travel",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.6, "places": [{"display_name": "Tokyo Skytree", "country": "Japan"}]}"#));
    assert_eq!(h.pipeline.process(&id).await, ProcessingStatus::NeedsReview);
    let calls = h.ai.calls.load(Ordering::SeqCst); // includes the single Level-3 escalation
    assert!(calls <= 2);
    let review = h.db.open_reviews().unwrap().into_iter().find(|r| r.kind == ReviewKind::TravelClassification).unwrap();
    h.pipeline.resolve_review(&review.id, "yes", None).await.unwrap();
    assert_eq!(h.ai.calls.load(Ordering::SeqCst), calls, "confirming must not call the AI again");
    assert_eq!(h.places().len(), 1);
}

#[tokio::test]
async fn merge_split_and_cascades() {
    let h = Harness::new();
    let a = h.screenshot("m1", "2025-01-01T00:00:00Z", "Fushimi Inari\nThings to do in Kyoto",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Fushimi Inari", "country": "Japan", "facts": [{"type": "tip", "text": "Wear good shoes"}]}]}"#));
    let b = h.screenshot("m2", "2025-01-02T00:00:00Z", "Kiyomizu-dera\nThings to do in Kyoto",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Kiyomizu-dera", "country": "Japan"}]}"#));
    h.pipeline.process(&a).await;
    h.pipeline.process(&b).await;
    let places = h.places();
    assert_eq!(places.len(), 2);
    let (fushimi, kiyomizu) = if places[0].canonical_name.starts_with("Fushimi") { (&places[0], &places[1]) } else { (&places[1], &places[0]) };

    merge::merge_places(&h.db, &kiyomizu.id, &fushimi.id).unwrap();
    let merged = h.db.place(&fushimi.id).unwrap().unwrap();
    assert_eq!(merged.source_count, 2);
    assert!(merged.alternative_names.contains(&"Kiyomizu-dera".to_string()));

    let new_id = merge::split_place(&h.db, &fushimi.id, &[b.clone()],
        &cand("Kiyomizu-dera", 34.9949, 135.7850, "JP", "Japan", "Kyoto", "m-kiyomizu")).unwrap();
    assert_eq!(h.db.place(&fushimi.id).unwrap().unwrap().source_count, 1);
    assert_eq!(h.db.place(&new_id).unwrap().unwrap().source_count, 1);

    // Deleting a place removes its links and facts but never the screenshots.
    h.db.delete_place(&fushimi.id).unwrap();
    assert!(h.db.facts_for_screenshot(&a).unwrap().is_empty());
    assert!(h.db.screenshot(&a).unwrap().is_some());
}

#[tokio::test]
async fn summary_uses_only_saved_facts() {
    let h = Harness::new();
    let id = h.screenshot("sum", "2025-01-01T00:00:00Z", "Fushimi Inari\nThings to do in Kyoto\nGo at sunrise",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Fushimi Inari", "country": "Japan", "facts": [{"type": "recommended_time", "text": "Go at sunrise"}]}]}"#));
    h.pipeline.process(&id).await;
    let place = &h.places()[0];
    let summary = h.pipeline.summarize_place(&place.id).await.unwrap();
    assert!(summary.starts_with("You saved"));
    let stored = h.db.place(&place.id).unwrap().unwrap();
    assert_eq!(stored.summary_fact_ids.len(), 1);
}

// MARK: Reels

struct MockMedia {
    words: Mutex<Vec<(String, f64)>>,
    /// (time, OCR text) per key frame.
    frames: Mutex<Vec<(f64, String)>>,
}

#[async_trait]
impl MediaService for MockMedia {
    async fn extract_audio(&self, _: &Path, out: &Path) -> Result<(Option<PathBuf>, f64)> {
        std::fs::create_dir_all(out.parent().unwrap())?;
        std::fs::write(out, b"m4a")?;
        Ok((Some(out.to_path_buf()), 20.0))
    }
    async fn transcribe(&self, _: &Path, locale: &str) -> Result<Transcription> {
        let words = self.words.lock().unwrap().iter()
            .map(|(t, s)| WordTiming { text: t.clone(), start: *s, duration: 0.3, confidence: 0.9 }).collect();
        Ok(Transcription { words, on_device: true, locale: locale.into(), engine: "mock".into() })
    }
    async fn keyframes(&self, _: &Path, out_dir: &Path, _: u32) -> Result<(f64, Vec<FrameInfo>)> {
        std::fs::create_dir_all(out_dir)?;
        let mut out = vec![];
        for (i, (time, _)) in self.frames.lock().unwrap().iter().enumerate() {
            let path = out_dir.join(format!("frame{i}.jpg"));
            image::RgbImage::from_pixel(90, 160, image::Rgb([240, 240, 240])).save(&path)?;
            out.push(FrameInfo { time_sec: *time, path: path.to_string_lossy().to_string() });
        }
        Ok((20.0, out))
    }
    async fn list_videos(&self, _: u32) -> Result<Vec<PhotoVideo>> { Ok(vec![]) }
    async fn speech_locales(&self) -> Result<Vec<SpeechLocale>> {
        Ok(vec![SpeechLocale { id: "en-US".into(), name: "English".into(), on_device: true, engine: "mock".into() }])
    }
    async fn detect_language(&self, _: &str) -> Result<Vec<(String, f64)>> { Ok(vec![("en".into(), 0.9)]) }
    async fn export_video(&self, _: &str, out: &Path) -> Result<Option<String>> {
        std::fs::create_dir_all(out.parent().unwrap())?;
        std::fs::write(out, b"mp4")?;
        Ok(Some("2025-06-01T00:00:00Z".into()))
    }
}

struct MockFetcher {
    caption: Mutex<Option<String>>,
    has_video: AtomicUsize,
}

#[async_trait]
impl ReelFetcher for MockFetcher {
    async fn metadata(&self, _: &str, _: &AppConfig) -> Result<ReelMetadata> {
        Ok(ReelMetadata { creator: Some("@fujiguide".into()), caption: self.caption.lock().unwrap().clone(), ..Default::default() })
    }
    async fn download(&self, _: &str, _: &ReelMetadata, out: &Path, _: &AppConfig) -> Result<Option<String>> {
        if self.has_video.load(Ordering::SeqCst) == 0 {
            return Ok(None);
        }
        std::fs::create_dir_all(out.parent().unwrap())?;
        std::fs::write(out, b"mp4")?;
        Ok(Some("ytdlp".into()))
    }
    async fn download_file(&self, _: &str, _: &Path) -> Result<()> { Ok(()) }
    fn yt_dlp(&self, _: &AppConfig) -> Option<PathBuf> { None }
}

impl Harness {
    /// Key frames' OCR is looked up by file stem, like screenshots.
    fn reel_frames(&self, frames: &[(f64, &str)]) {
        *self.media.frames.lock().unwrap() = frames.iter().map(|(t, s)| (*t, s.to_string())).collect();
        for (i, (_, text)) in frames.iter().enumerate() {
            self.ocr.texts.lock().unwrap().insert(format!("frame{i}"), text.to_string());
        }
    }

    fn reel_words(&self, sentence_starts: &[(&str, f64)]) {
        let mut words = vec![];
        for (sentence, start) in sentence_starts {
            for (i, w) in sentence.split(' ').enumerate() {
                words.push((w.to_string(), start + i as f64 * 0.3));
            }
        }
        *self.media.words.lock().unwrap() = words;
    }
}

const REEL_AI: &str = r#"{"is_travel_related": true, "travel_confidence": 0.96, "source_type": "instagram",
  "places": [
    {"display_name": "Lake Kawaguchi", "country": "Japan", "category": "nature",
     "facts": [{"type": "recommended_time", "text": "Come early in the morning for clearer Fuji views", "source_lines": [1]},
               {"type": "transportation", "text": "Take the train from Shinjuku", "source_lines": [2]}]},
    {"display_name": "Chureito Pagoda", "country": "Japan", "category": "viewpoint",
     "facts": [{"type": "price", "text": "¥500 entry", "source_lines": [4]}]}
  ]}"#;

#[tokio::test]
async fn reel_audio_first_extracts_places_with_timestamps() {
    let h = Harness::new();
    *h.fetcher.caption.lock().unwrap() = Some("Fuji day trip 🗻".into());
    h.reel_words(&[("This is Lake Kawaguchi.", 3.0), ("Come early in the morning for clearer Fuji views.", 7.0), ("Take the train from Shinjuku.", 13.0)]);
    h.reel_frames(&[(5.0, "Chureito Pagoda\n¥500 entry"), (15.0, "Follow")]);
    h.ai.responses.lock().unwrap().insert("audio 00:03: This is Lake Kawaguchi.".into(), AiBehaviour::Json(REEL_AI.into()));

    let (id, created) = h.pipeline.register_reel("https://www.instagram.com/reel/ABC123/?igsh=x").unwrap();
    assert!(created);
    assert_eq!(h.pipeline.process_reel(&id).await, ProcessingStatus::Complete);

    let reel = h.db.reel(&id).unwrap().unwrap();
    assert_eq!(reel.transcript.len(), 3);
    assert_eq!(reel.creator.as_deref(), Some("@fujiguide"));
    assert!(reel.audio_path.is_some(), "voice audio is saved");
    assert_eq!(h.db.keyframes(&id).unwrap().len(), 2, "key snapshots are saved");
    assert_eq!(h.ai.calls.load(Ordering::SeqCst), 1, "one DeepSeek call per Reel");

    let places = h.db.places_for_reel(&id).unwrap();
    assert_eq!(places.len(), 2, "one Reel, several places");
    let lake = places.iter().find(|p| p.canonical_name == "Lake Kawaguchi").unwrap();
    let facts = h.db.facts_for_place(&lake.id).unwrap();
    let early = facts.iter().find(|f| f.text.starts_with("Come early")).unwrap();
    assert_eq!(early.source_kind, "audio");
    assert_eq!(early.source_time_sec, Some(7.0));
    assert_eq!(early.reel_id.as_deref(), Some(id.as_str()));
    let pagoda = places.iter().find(|p| p.canonical_name == "Chureito Pagoda").unwrap();
    let price = &h.db.facts_for_place(&pagoda.id).unwrap()[0];
    assert_eq!((price.source_kind.as_str(), price.source_time_sec), ("keyframe", Some(5.0)));

    // Pasting the same Reel again doesn't create a duplicate.
    let (again, created) = h.pipeline.register_reel("https://instagram.com/reel/ABC123/").unwrap();
    assert_eq!((again.as_str(), created), (id.as_str(), false));
}

#[tokio::test]
async fn reel_merges_into_place_saved_from_a_screenshot() {
    let h = Harness::new();
    let shot = h.screenshot("k", "2025-01-01T00:00:00Z", "Lake Kawaguchi\nThings to do in Japan\nBest at sunrise",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Lake Kawaguchi", "country": "Japan",
          "facts": [{"type": "recommended_time", "text": "Best at sunrise"}]}]}"#));
    h.pipeline.process(&shot).await;

    h.reel_words(&[("This is Lake Kawaguchi.", 3.0), ("Come early in the morning for clearer Fuji views.", 7.0), ("Take the train from Shinjuku.", 13.0)]);
    h.reel_frames(&[(5.0, "Chureito Pagoda\n¥500 entry")]);
    h.ai.responses.lock().unwrap().insert("audio 00:03: This is Lake Kawaguchi.".into(), AiBehaviour::Json(REEL_AI.into()));
    let (id, _) = h.pipeline.register_reel("https://www.instagram.com/reel/XYZ/").unwrap();
    h.pipeline.process_reel(&id).await;

    let lakes: Vec<_> = h.places().into_iter().filter(|p| p.canonical_name == "Lake Kawaguchi").collect();
    assert_eq!(lakes.len(), 1, "no duplicate place");
    assert_eq!(lakes[0].source_count, 2, "screenshot + Reel");
    let kinds: Vec<String> = h.db.facts_for_place(&lakes[0].id).unwrap().into_iter().map(|f| f.source_kind).collect();
    assert!(kinds.contains(&"screenshot".to_string()) && kinds.contains(&"audio".to_string()));
}

#[tokio::test]
async fn reel_without_media_or_caption_asks_for_video_and_skips_ai() {
    let h = Harness::new();
    h.fetcher.has_video.store(0, Ordering::SeqCst);
    let (id, _) = h.pipeline.register_reel("https://www.instagram.com/reel/NOPE/").unwrap();
    assert_eq!(h.pipeline.process_reel(&id).await, ProcessingStatus::NeedsMedia);
    assert_eq!(h.ai.calls.load(Ordering::SeqCst), 0);

    // Fallback: attach a video from Photos, then it processes normally.
    h.reel_words(&[("This is Lake Kawaguchi.", 3.0), ("Come early in the morning for clearer Fuji views.", 7.0), ("Take the train from Shinjuku.", 13.0)]);
    h.ai.responses.lock().unwrap().insert("audio 00:03: This is Lake Kawaguchi.".into(), AiBehaviour::Json(REEL_AI.into()));
    h.pipeline.attach_photos_video(Some(&id), "photos-asset-1").await.unwrap();
    assert_eq!(h.pipeline.process_reel(&id).await, ProcessingStatus::Complete);
    assert_eq!(h.db.reel(&id).unwrap().unwrap().media_source.as_deref(), Some("photos"));
}

#[test]
fn rejects_non_instagram_links() {
    let h = Harness::new();
    assert!(h.pipeline.register_reel("https://example.com/video").is_err());
}

// MARK: Live (opt-in) — real Vision OCR, DeepSeek and MapKit. Run with:
//   TSM_LIVE_IMAGE=/path/to/screenshot.jpg cargo test live_ -- --ignored --nocapture

struct FilePhotos(PathBuf);

#[async_trait]
impl PhotoLibraryService for FilePhotos {
    async fn authorization_status(&self) -> Result<String> { Ok("authorized".into()) }
    async fn request_permission(&self) -> Result<String> { Ok("authorized".into()) }
    async fn list_screenshots(&self, _: Option<i32>, _: Option<i32>) -> Result<Vec<AssetInfo>> { Ok(vec![]) }
    async fn metadata(&self, _: &str) -> Result<Value> { Ok(Value::Null) }
    async fn export_image(&self, _: &str, path: &Path, _: u32) -> Result<()> {
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::copy(&self.0, path)?;
        Ok(())
    }
    async fn crop_image(&self, source: &Path, dest: &Path, _: Rect, _: u32) -> Result<()> {
        std::fs::create_dir_all(dest.parent().unwrap())?;
        std::fs::copy(source, dest)?;
        Ok(())
    }
    async fn observe_new_screenshots(&self) -> Result<()> { Ok(()) }
}

#[tokio::test]
#[ignore]
async fn live_screenshot_end_to_end() {
    for f in ["../.env.local", ".env.local"] {
        let _ = dotenvy::from_filename(f);
    }
    let image = PathBuf::from(std::env::var("TSM_LIVE_IMAGE").expect("set TSM_LIVE_IMAGE"));
    let bridge = Arc::new(NativeBridge::new(NativeBridge::locate_binary()));
    let config = AppConfig::default();
    let (key, _) = crate::config::resolve_api_key();
    let ai = Arc::new(crate::services::ai::deepseek::DeepSeekTravelAIService::new(config.clone(), key));
    let db = Arc::new(Database::open_in_memory().unwrap());
    let dir = std::env::temp_dir().join(format!("tsm-live-{}", uuid::Uuid::new_v4()));
    let pipeline = Pipeline::new(db.clone(), Arc::new(FilePhotos(image)), bridge.clone(), bridge.clone(),
        Arc::new(PlaceService::new(bridge.clone())), bridge.clone(), Arc::new(MockFetcher { caption: Mutex::new(None), has_video: AtomicUsize::new(0) }),
        ai, config, dir);
    db.upsert_discovered(&[AssetInfo { id: "live".into(), creation_date: Some("2025-05-01T00:00:00Z".into()), width: 0, height: 0 }]).unwrap();
    let id: String = db.with(|c| c.query_row("SELECT id FROM screenshots", [], |r| r.get(0))).unwrap();

    let status = pipeline.process(&id).await;
    let shot = db.screenshot(&id).unwrap().unwrap();
    println!("status: {status:?} ({:?})", shot.status_detail);
    println!("ocr: {:?}", shot.ocr_full_text);
    println!("ai: {:?} in={} out={} cost=${:.6}", shot.ai_model, shot.ai_input_tokens, shot.ai_output_tokens, shot.ai_cost);
    for p in db.list_places(&PlaceFilter::default()).unwrap() {
        println!("place: {} — {:?}, {:?} ({:.4}, {:.4}) [{}]", p.canonical_name, p.city, p.country, p.latitude, p.longitude, p.verification);
        for f in db.facts_for_place(&p.id).unwrap() {
            println!("   fact [{}] {} (blocks: {})", f.fact_type, f.text, f.source_block_ids.len());
        }
    }
    for r in db.open_reviews().unwrap() {
        println!("review: {} — {}", r.kind, r.message);
    }
    assert!(matches!(status, ProcessingStatus::Complete | ProcessingStatus::NeedsReview));
}

/// Real-data validation: random sample of the user's actual screenshots through the full pipeline.
///   TSM_LIVE_COUNT=100 TSM_LIVE_DB=/tmp/x.sqlite cargo test live_photos -- --ignored --nocapture
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn live_photos_validation() {
    for f in ["../.env.local", ".env.local"] {
        let _ = dotenvy::from_filename(f);
    }
    let count: usize = std::env::var("TSM_LIVE_COUNT").ok().and_then(|c| c.parse().ok()).unwrap_or(10);
    let db_path = PathBuf::from(std::env::var("TSM_LIVE_DB").expect("set TSM_LIVE_DB"));
    let data_dir = db_path.parent().unwrap().join("data");
    let bridge = Arc::new(NativeBridge::new(NativeBridge::locate_binary()));
    let mut config = AppConfig::default();
    config.max_concurrent_screenshots = 3;
    let (key, _) = crate::config::resolve_api_key();
    let ai = Arc::new(crate::services::ai::deepseek::DeepSeekTravelAIService::new(config.clone(), key));
    let db = Arc::new(Database::open(&db_path).unwrap());
    let pipeline = Arc::new(Pipeline::new(db.clone(), bridge.clone(), bridge.clone(), bridge.clone(),
        Arc::new(PlaceService::new(bridge.clone())), bridge.clone(), Arc::new(crate::services::reels::InstagramFetcher::default()),
        ai, config, data_dir));
    let queue = Arc::new(crate::pipeline::queue::ProcessingQueue::new(pipeline.clone(), Arc::new(|s: &crate::pipeline::queue::QueueSnapshot| {
        if s.processed % 10 == 0 && s.processed > 0 { eprintln!("  … {}/{} processed", s.processed, s.total); }
    })));
    let started = std::time::Instant::now();
    let run_id = if std::env::var("TSM_LIVE_REPROCESS").is_ok() {
        // Re-run the same screenshots (OCR is reused) to compare before/after a fix.
        let ids: Vec<String> = db.with(|c| { let mut s = c.prepare("SELECT id FROM screenshots")?; let r = s.query_map([], |r| r.get(0))?; r.collect() }).unwrap();
        db.mark_for_reprocess("all", &[]).unwrap();
        let run = db.create_run("validation", ids.len() as i64, "rerun").unwrap();
        db.set_run_screenshots(&run, &ids).unwrap();
        queue.clone().run(crate::pipeline::queue::RunMode::Retry).await;
        db.finish_run(&run).unwrap();
        run
    } else {
        queue.clone().run(crate::pipeline::queue::RunMode::Validation { count, random: true }).await.expect("run id")
    };
    let report = db.run_report(&run_id).unwrap().unwrap();
    println!("\n=== VALIDATION: {} screenshots in {:.0}s ===", report.total, started.elapsed().as_secs_f64());
    println!("travel {} | not travel {} (skipped locally {}) | needs review {} | failed {} | waiting {}",
        report.travel, report.not_travel, report.skipped_locally, report.needs_review, report.failed, report.waiting);
    println!("AI requests {} | places found {} | extracted {} | auto-resolved {} ({:.0}%)",
        report.ai_requests, report.places_found, report.places_extracted, report.places_auto_resolved, report.resolution_rate * 100.0);
    println!("avg OCR {:.0} ms | avg AI {:.0} ms | avg total {:.0} ms | cost ${:.4} (${:.5}/travel) | tokens {} in / {} out",
        report.avg_ocr_ms, report.avg_ai_ms, report.avg_total_ms, report.total_cost, report.cost_per_travel_screenshot, report.input_tokens, report.output_tokens);
    for r in &report.rows {
        let shot = db.screenshot(&r.screenshot_id).unwrap().unwrap();
        let snippet: String = shot.ocr_full_text.replace('\n', " / ").chars().take(110).collect();
        println!("[{:<11}] L{} p={:.2} local={:.2} {:<10} places={:?} {} | {}", r.status, r.escalation_level, r.travel_confidence,
            shot.local_travel_score, r.source_type, r.places, r.status_detail.clone().unwrap_or_default(), snippet);
    }
    for rv in db.open_reviews().unwrap() {
        println!("REVIEW {}: {} | candidates: {:?}", rv.kind, rv.message, rv.candidates.iter().map(|c| format!("{} ({})", c.name, c.country.clone().unwrap_or_default())).collect::<Vec<_>>());
    }
}

/// Real Instagram Reels through the full Reel pipeline.
///   TSM_LIVE_REELS=urls.txt TSM_LIVE_DB=/tmp/r.sqlite cargo test live_reels -- --ignored --nocapture
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn live_reels_validation() {
    for f in ["../.env.local", ".env.local"] {
        let _ = dotenvy::from_filename(f);
    }
    let urls: Vec<String> = std::fs::read_to_string(std::env::var("TSM_LIVE_REELS").expect("set TSM_LIVE_REELS")).unwrap()
        .lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).map(String::from).collect();
    let db_path = PathBuf::from(std::env::var("TSM_LIVE_DB").expect("set TSM_LIVE_DB"));
    let bridge = Arc::new(NativeBridge::new(NativeBridge::locate_binary()));
    let config = AppConfig::default();
    let (key, _) = crate::config::resolve_api_key();
    let ai = Arc::new(crate::services::ai::deepseek::DeepSeekTravelAIService::new(config.clone(), key));
    let db = Arc::new(Database::open(&db_path).unwrap());
    let pipeline = Pipeline::new(db.clone(), bridge.clone(), bridge.clone(), bridge.clone(),
        Arc::new(PlaceService::new(bridge.clone())), bridge.clone(), Arc::new(crate::services::reels::InstagramFetcher::default()),
        ai, config, db_path.parent().unwrap().join("data"));

    let (mut total_cost, mut places_total, mut ok) = (0.0, 0, 0);
    for url in &urls {
        let started = std::time::Instant::now();
        let (id, _) = pipeline.register_reel(url).unwrap();
        let status = pipeline.process_reel(&id).await;
        let reel = db.reel(&id).unwrap().unwrap();
        total_cost += reel.ai_cost;
        println!("\n### {url}\nstatus {status:?} in {:.0}s · {} · cost ${:.5} · {:?}", started.elapsed().as_secs_f64(),
            reel.creator.clone().unwrap_or_default(), reel.ai_cost, reel.status_detail);
        for s in crate::pipeline::reels::STAGES {
            let st = &reel.stages[s];
            println!("  {:<10} {:<8} {}", s, st["status"].as_str().unwrap_or("-"), st["detail"].as_str().unwrap_or(""));
        }
        for seg in reel.transcript.iter().take(3) {
            println!("  🎙 {:>5.1}s {}", seg.start, seg.text.chars().take(100).collect::<String>());
        }
        let places = db.places_for_reel(&id).unwrap();
        places_total += places.len();
        if matches!(status, ProcessingStatus::Complete | ProcessingStatus::NeedsReview) { ok += 1; }
        for p in &places {
            let facts = db.facts_for_place(&p.id).unwrap().into_iter().filter(|f| f.reel_id.as_deref() == Some(&id)).collect::<Vec<_>>();
            println!("  📍 {} — {} [{}] facts: {}", p.canonical_name, p.subtitle(), p.verification,
                facts.iter().map(|f| format!("{}@{}:{}", f.fact_type, f.source_kind, f.source_time_sec.map(|t| format!("{t:.0}s")).unwrap_or_default())).collect::<Vec<_>>().join(", "));
        }
        for r in db.reviews_for_reel(&id).unwrap() {
            println!("  ❓ {}: {}", r.kind, r.message);
        }
    }
    println!("\n=== REELS: {ok}/{} usable · {places_total} place links · total AI cost ${total_cost:.4} ===", urls.len());
}

#[tokio::test]
async fn choosing_a_place_in_review_keeps_reel_timestamps() {
    let h = Harness::new();
    h.reel_words(&[("We swam at the Blue Lagoon.", 3.0), ("Book tickets a week ahead.", 8.0)]);
    h.ai.responses.lock().unwrap().insert("audio 00:03: We swam at the Blue Lagoon.".into(), AiBehaviour::Json(
        r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Blue Lagoon", "ambiguous": true,
            "facts": [{"type": "reservation", "text": "Book tickets a week ahead", "source_lines": [1]}]}]}"#.into()));
    let (id, _) = h.pipeline.register_reel("https://www.instagram.com/reel/BLUE1/").unwrap();
    assert_eq!(h.pipeline.process_reel(&id).await, ProcessingStatus::NeedsReview);

    let review = h.db.reviews_for_reel(&id).unwrap().into_iter().find(|r| r.kind == ReviewKind::PlaceResolution).unwrap();
    let iceland = review.candidates.iter().find(|c| c.country_code.as_deref() == Some("IS")).unwrap().clone();
    h.pipeline.resolve_review(&review.id, "choose", Some(iceland)).await.unwrap();

    let places = h.db.places_for_reel(&id).unwrap();
    assert_eq!(places.len(), 1);
    assert_eq!(places[0].country_code.as_deref(), Some("IS"));
    let fact = &h.db.facts_for_place(&places[0].id).unwrap()[0];
    assert_eq!((fact.source_kind.as_str(), fact.source_time_sec), ("audio", Some(8.0)), "provenance survives the correction");
    assert_eq!(h.db.reel(&id).unwrap().unwrap().status, ProcessingStatus::Complete);
}

// MARK: Corrections survive re-processing

const KYOTO_TEXT: &str = "5 places you need to visit in Kyoto\n1. Fushimi Inari\nGo before 8 AM to avoid crowds\n2. Kiyomizu-dera\n3. Nishiki Market\n4. Arashiyama Bamboo Grove";

#[tokio::test]
async fn removed_pin_stays_removed_after_reprocessing() {
    let h = Harness::new();
    let id = h.screenshot("rm", "2025-03-01T00:00:00Z", KYOTO_TEXT, json(KYOTO));
    h.pipeline.process(&id).await;
    let nishiki = h.places().into_iter().find(|p| p.canonical_name == "Nishiki Market").unwrap();

    h.pipeline.remove_place_from_source(&nishiki.id, Some(&id), None).unwrap();
    assert_eq!(h.db.links_for_screenshot(&id).unwrap().len(), 3);

    h.pipeline.reprocess(&id).await.unwrap();
    let linked: Vec<String> = h.db.places_for_screenshot(&id).unwrap().into_iter().map(|p| p.canonical_name).collect();
    assert_eq!(linked.len(), 3, "{linked:?}");
    assert!(!linked.contains(&"Nishiki Market".to_string()), "the removed pin came back: {linked:?}");
}

#[tokio::test]
async fn not_a_place_answer_survives_reprocessing() {
    let h = Harness::new();
    let id = h.screenshot("np", "2025-01-01T00:00:00Z", "Blue Lagoon\nMust visit! Travel goals",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.9, "places": [{"display_name": "Blue Lagoon", "ambiguous": true}]}"#));
    assert_eq!(h.pipeline.process(&id).await, ProcessingStatus::NeedsReview);
    let review = h.db.open_reviews().unwrap().into_iter().find(|r| r.kind == ReviewKind::PlaceResolution).unwrap();
    h.pipeline.resolve_review(&review.id, "dismiss", None).await.unwrap();

    assert_eq!(h.pipeline.reprocess(&id).await.unwrap(), ProcessingStatus::Complete);
    assert!(h.db.open_reviews().unwrap().is_empty(), "the same question was asked again");
    assert!(h.places().is_empty());
    // The answer is still listed under Recently reviewed.
    let answered = h.db.reviews_for_screenshot(&id).unwrap();
    assert!(answered.iter().any(|r| r.is_resolved && r.resolution.as_deref() == Some("not a place")));
}

#[tokio::test]
async fn merged_places_stay_merged_after_reprocessing() {
    let h = Harness::new();
    let a = h.screenshot("mg1", "2025-01-01T00:00:00Z", "Fushimi Inari\nThings to do in Kyoto",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Fushimi Inari", "country": "Japan"}]}"#));
    let b = h.screenshot("mg2", "2025-01-02T00:00:00Z", "Kiyomizu-dera\nThings to do in Kyoto",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Kiyomizu-dera", "country": "Japan"}]}"#));
    h.pipeline.process(&a).await;
    h.pipeline.process(&b).await;
    let places = h.places();
    let fushimi = places.iter().find(|p| p.canonical_name.starts_with("Fushimi")).unwrap().clone();
    let kiyomizu = places.iter().find(|p| p.canonical_name == "Kiyomizu-dera").unwrap().clone();

    merge::merge_places(&h.db, &kiyomizu.id, &fushimi.id).unwrap();
    h.pipeline.reprocess(&b).await.unwrap();

    let places = h.places();
    assert_eq!(places.len(), 1, "the merged place was recreated: {:?}", places.iter().map(|p| &p.canonical_name).collect::<Vec<_>>());
    assert_eq!(h.db.places_for_screenshot(&b).unwrap()[0].id, fushimi.id);
}

#[tokio::test]
async fn corrected_location_survives_reprocessing() {
    let h = Harness::new();
    let id = h.screenshot("loc", "2025-01-01T00:00:00Z", "Tokyo Skytree\nThings to do in Japan",
        json(r#"{"is_travel_related": true, "travel_confidence": 0.95, "places": [{"display_name": "Tokyo Skytree", "country": "Japan"}]}"#));
    h.pipeline.process(&id).await;
    let place = h.places()[0].clone();

    // The user moves the pin to the right spot (a different Apple Maps entry ~5 km away).
    let right = cand("Tokyo Skytree Town", 35.7400, 139.8400, "JP", "Japan", "Tokyo", "m-skytree-town");
    assert_eq!(merge::relocate_place(&h.db, &place.id, &right).unwrap(), place.id);

    h.pipeline.reprocess(&id).await.unwrap();
    let places = h.places();
    assert_eq!(places.len(), 1, "the wrong pin came back");
    assert_eq!((places[0].latitude, places[0].map_identifier.as_deref()), (35.7400, Some("m-skytree-town")));
    assert_eq!(h.db.places_for_screenshot(&id).unwrap()[0].id, place.id);
}

#[tokio::test]
async fn reprocessing_twice_creates_no_duplicates() {
    let h = Harness::new();
    let id = h.screenshot("idem", "2025-03-01T00:00:00Z", KYOTO_TEXT, json(KYOTO));
    h.pipeline.process(&id).await;
    let before: Vec<(String, usize)> = h.places().iter().map(|p| (p.canonical_name.clone(), h.db.facts_for_place(&p.id).unwrap().len())).collect();
    h.pipeline.reprocess(&id).await.unwrap();
    h.pipeline.reprocess(&id).await.unwrap();
    let mut after: Vec<(String, usize)> = h.places().iter().map(|p| (p.canonical_name.clone(), h.db.facts_for_place(&p.id).unwrap().len())).collect();
    let mut before = before;
    before.sort();
    after.sort();
    assert_eq!(before, after);
    assert_eq!(h.ai.calls.load(Ordering::SeqCst), 1, "re-processing reuses the cached AI result");
}
