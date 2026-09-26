//! Row types returned to the pipeline and the UI (serialised as camelCase JSON).

use rusqlite::Row;
use serde::{Deserialize, Serialize};

use crate::models::*;

fn json_vec(s: String) -> Vec<String> {
    serde_json::from_str(&s).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceRecord {
    pub id: String,
    pub canonical_name: String,
    pub alternative_names: Vec<String>,
    pub map_identifier: Option<String>,
    pub latitude: f64,
    pub longitude: f64,
    pub address: Option<String>,
    pub city: Option<String>,
    pub region: Option<String>,
    pub country: Option<String>,
    pub country_code: Option<String>,
    pub category: PlaceCategory,
    pub verification: Verification,
    pub origin: DataOrigin,
    pub is_user_verified: bool,
    pub personal_status: PersonalStatus,
    pub notes: String,
    pub summary_text: Option<String>,
    pub summary_fact_ids: Vec<String>,
    pub summary_generated_at: Option<String>,
    pub hero_image_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    // Derived
    pub source_count: i64,
    pub hero_image_path: Option<String>,
    pub thumbnail_path: Option<String>,
    // After the trip
    pub visited_at: Option<String>,
    pub visit_notes: String,
    pub cover_memory_id: Option<String>,
    pub memory_count: i64,
}

pub const PLACE_COLUMNS: &str = "p.id, p.canonical_name, p.alternative_names, p.map_identifier, p.latitude, p.longitude, \
    p.address, p.city, p.region, p.country, p.country_code, p.category, p.verification, p.origin, p.is_user_verified, \
    p.personal_status, p.notes, p.summary_text, p.summary_fact_ids, p.summary_generated_at, p.hero_image_id, \
    p.created_at, p.updated_at, \
    (SELECT COUNT(*) FROM place_screenshots ps WHERE ps.place_id = p.id) + (SELECT COUNT(*) FROM place_reels pr WHERE pr.place_id = p.id), \
    COALESCE((SELECT m.image_path FROM place_memories m WHERE m.id = p.cover_memory_id), \
             (SELECT i.image_path FROM place_images i WHERE i.id = p.hero_image_id AND i.is_accepted = 1), \
             (SELECT i.image_path FROM place_images i WHERE i.place_id = p.id AND i.is_accepted = 1 ORDER BY i.quality_score DESC LIMIT 1)), \
    COALESCE((SELECT s.thumbnail_path FROM place_screenshots ps JOIN screenshots s ON s.id = ps.screenshot_id \
       WHERE ps.place_id = p.id AND s.thumbnail_path IS NOT NULL ORDER BY s.creation_date DESC LIMIT 1), \
    (SELECT rl.thumbnail_path FROM place_reels pr JOIN reels rl ON rl.id = pr.reel_id WHERE pr.place_id = p.id AND rl.thumbnail_path IS NOT NULL LIMIT 1)), \
    p.visited_at, p.visit_notes, p.cover_memory_id, (SELECT COUNT(*) FROM place_memories m WHERE m.place_id = p.id)";

impl PlaceRecord {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            canonical_name: r.get(1)?,
            alternative_names: json_vec(r.get(2)?),
            map_identifier: r.get(3)?,
            latitude: r.get(4)?,
            longitude: r.get(5)?,
            address: r.get(6)?,
            city: r.get(7)?,
            region: r.get(8)?,
            country: r.get(9)?,
            country_code: r.get(10)?,
            category: r.get(11)?,
            verification: r.get(12)?,
            origin: r.get(13)?,
            is_user_verified: r.get(14)?,
            personal_status: r.get(15)?,
            notes: r.get(16)?,
            summary_text: r.get(17)?,
            summary_fact_ids: json_vec(r.get(18)?),
            summary_generated_at: r.get(19)?,
            hero_image_id: r.get(20)?,
            created_at: r.get(21)?,
            updated_at: r.get(22)?,
            source_count: r.get(23)?,
            hero_image_path: r.get(24)?,
            thumbnail_path: r.get(25)?,
            visited_at: r.get(26)?,
            visit_notes: r.get(27)?,
            cover_memory_id: r.get(28)?,
            memory_count: r.get(29)?,
        })
    }

    pub fn subtitle(&self) -> String {
        [self.city.clone(), self.country.clone()].into_iter().flatten().collect::<Vec<_>>().join(", ")
    }

    pub fn all_names(&self) -> Vec<String> {
        let mut v = vec![self.canonical_name.clone()];
        v.extend(self.alternative_names.iter().cloned());
        v
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotRecord {
    pub id: String,
    pub photos_id: String,
    pub creation_date: Option<String>,
    pub width: i64,
    pub height: i64,
    pub discovered_at: String,
    pub processed_at: Option<String>,
    pub processing_version: i64,
    pub ocr_version: i64,
    pub ai_prompt_version: i64,
    pub status: ProcessingStatus,
    pub status_detail: Option<String>,
    pub failure_count: i64,
    pub classification: Classification,
    pub user_classification: Option<String>,
    pub travel_confidence: f64,
    pub local_travel_score: f64,
    pub source_type: SourceType,
    pub creator: Option<String>,
    pub ocr_full_text: String,
    pub ocr_hash: Option<String>,
    pub image_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub escalation_level: i64,
    pub ai_model: Option<String>,
    pub ai_thinking: bool,
    pub ai_image_used: bool,
    pub ai_input_tokens: i64,
    pub ai_output_tokens: i64,
    pub ai_latency_ms: i64,
    pub ai_retry_count: i64,
    pub ai_cost: f64,
    pub place_count: i64,
    pub open_review_count: i64,
}

pub const SCREENSHOT_COLUMNS: &str = "s.id, s.photos_id, s.creation_date, s.width, s.height, s.discovered_at, s.processed_at, \
    s.processing_version, s.ocr_version, s.ai_prompt_version, s.status, s.status_detail, s.failure_count, s.classification, \
    s.user_classification, s.travel_confidence, s.local_travel_score, s.source_type, s.creator, s.ocr_full_text, s.ocr_hash, \
    s.image_path, s.thumbnail_path, s.escalation_level, s.ai_model, s.ai_thinking, s.ai_image_used, s.ai_input_tokens, \
    s.ai_output_tokens, s.ai_latency_ms, s.ai_retry_count, s.ai_cost, \
    (SELECT COUNT(*) FROM place_screenshots ps WHERE ps.screenshot_id = s.id), \
    (SELECT COUNT(*) FROM review_items r WHERE r.screenshot_id = s.id AND r.is_resolved = 0)";

impl ScreenshotRecord {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            photos_id: r.get(1)?,
            creation_date: r.get(2)?,
            width: r.get(3)?,
            height: r.get(4)?,
            discovered_at: r.get(5)?,
            processed_at: r.get(6)?,
            processing_version: r.get(7)?,
            ocr_version: r.get(8)?,
            ai_prompt_version: r.get(9)?,
            status: r.get(10)?,
            status_detail: r.get(11)?,
            failure_count: r.get(12)?,
            classification: r.get(13)?,
            user_classification: r.get(14)?,
            travel_confidence: r.get(15)?,
            local_travel_score: r.get(16)?,
            source_type: r.get(17)?,
            creator: r.get(18)?,
            ocr_full_text: r.get(19)?,
            ocr_hash: r.get(20)?,
            image_path: r.get(21)?,
            thumbnail_path: r.get(22)?,
            escalation_level: r.get(23)?,
            ai_model: r.get(24)?,
            ai_thinking: r.get(25)?,
            ai_image_used: r.get(26)?,
            ai_input_tokens: r.get(27)?,
            ai_output_tokens: r.get(28)?,
            ai_latency_ms: r.get(29)?,
            ai_retry_count: r.get(30)?,
            ai_cost: r.get(31)?,
            place_count: r.get(32)?,
            open_review_count: r.get(33)?,
        })
    }

    pub fn user_classification(&self) -> Option<Classification> {
        self.user_classification.as_deref().and_then(Classification::try_parse)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrBlockRecord {
    pub id: String,
    pub idx: i64,
    pub text: String,
    pub confidence: f64,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub language: Option<String>,
}

impl OcrBlockRecord {
    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactRecord {
    pub id: String,
    pub place_id: String,
    pub screenshot_id: Option<String>,
    pub reel_id: Option<String>,
    /// screenshot | audio | caption | keyframe | user
    pub source_kind: String,
    /// Position in the Reel the fact came from.
    pub source_time_sec: Option<f64>,
    #[serde(rename = "type")]
    pub fact_type: TravelFactType,
    pub text: String,
    pub source_quote: Option<String>,
    pub confidence: f64,
    pub source_block_ids: Vec<String>,
    pub valid_from: Option<String>,
    pub origin: DataOrigin,
    pub created_at: String,
    pub source_type: Option<SourceType>,
    pub creator: Option<String>,
}

pub const FACT_COLUMNS: &str = "f.id, f.place_id, f.screenshot_id, f.type, f.text, f.source_quote, f.confidence, \
    f.source_block_ids, f.valid_from, f.origin, f.created_at, \
    CASE WHEN f.reel_id IS NOT NULL THEN 'instagram' ELSE s.source_type END, COALESCE(s.creator, rl.creator), \
    f.reel_id, f.source_kind, f.source_time_sec";

impl FactRecord {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            place_id: r.get(1)?,
            screenshot_id: r.get(2)?,
            fact_type: r.get(3)?,
            text: r.get(4)?,
            source_quote: r.get(5)?,
            confidence: r.get(6)?,
            source_block_ids: json_vec(r.get(7)?),
            valid_from: r.get(8)?,
            origin: r.get(9)?,
            created_at: r.get(10)?,
            source_type: r.get(11)?,
            creator: r.get(12)?,
            reel_id: r.get(13)?,
            source_kind: r.get(14)?,
            source_time_sec: r.get(15)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceImageRecord {
    pub id: String,
    pub place_id: String,
    pub screenshot_id: Option<String>,
    pub crop: Rect,
    pub image_path: Option<String>,
    pub quality_score: f64,
    pub region_type: RegionType,
    pub region_confidence: f64,
    pub is_accepted: bool,
    pub origin: DataOrigin,
    pub duplicate_source_ids: Vec<String>,
    pub created_at: String,
    #[serde(skip)]
    pub feature_print: Option<Vec<f32>>,
}

pub const IMAGE_COLUMNS: &str = "id, place_id, screenshot_id, crop_x, crop_y, crop_width, crop_height, image_path, \
    quality_score, region_type, region_confidence, is_accepted, origin, duplicate_source_ids, created_at, feature_print";

impl PlaceImageRecord {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        let blob: Option<Vec<u8>> = r.get(15)?;
        Ok(Self {
            id: r.get(0)?,
            place_id: r.get(1)?,
            screenshot_id: r.get(2)?,
            crop: Rect::new(r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?),
            image_path: r.get(7)?,
            quality_score: r.get(8)?,
            region_type: r.get(9)?,
            region_confidence: r.get(10)?,
            is_accepted: r.get(11)?,
            origin: r.get(12)?,
            duplicate_source_ids: json_vec(r.get(13)?),
            created_at: r.get(14)?,
            feature_print: blob.map(|b| b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkRecord {
    pub place_id: String,
    pub screenshot_id: String,
    pub extracted_name: String,
    pub confidence: f64,
    pub origin: DataOrigin,
    pub is_user_verified: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRecord {
    pub id: String,
    pub kind: ReviewKind,
    pub screenshot_id: Option<String>,
    pub reel_id: Option<String>,
    pub message: String,
    pub extracted_place: Option<serde_json::Value>,
    pub candidates: Vec<PlaceCandidate>,
    pub place_a_id: Option<String>,
    pub place_b_id: Option<String>,
    pub image_id: Option<String>,
    pub is_resolved: bool,
    pub resolution: Option<String>,
    pub created_at: String,
    pub screenshot_thumbnail: Option<String>,
}

pub const REVIEW_COLUMNS: &str = "r.id, r.kind, r.screenshot_id, r.message, r.extracted_place, r.candidates, r.place_a_id, \
    r.place_b_id, r.image_id, r.is_resolved, r.resolution, r.created_at, COALESCE(s.thumbnail_path, rl.thumbnail_path), r.reel_id";

impl ReviewRecord {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        let extracted: Option<String> = r.get(4)?;
        let candidates: String = r.get(5)?;
        Ok(Self {
            id: r.get(0)?,
            kind: r.get(1)?,
            screenshot_id: r.get(2)?,
            message: r.get(3)?,
            extracted_place: extracted.and_then(|e| serde_json::from_str(&e).ok()),
            candidates: serde_json::from_str(&candidates).unwrap_or_default(),
            place_a_id: r.get(6)?,
            place_b_id: r.get(7)?,
            image_id: r.get(8)?,
            is_resolved: r.get(9)?,
            resolution: r.get(10)?,
            created_at: r.get(11)?,
            screenshot_thumbnail: r.get(12)?,
            reel_id: r.get(13)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TripRecord {
    pub id: String,
    pub name: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub notes: String,
    pub created_at: String,
    pub place_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TripEntryRecord {
    pub id: String,
    pub trip_id: String,
    pub position: i64,
    pub day: Option<i64>,
    pub place: PlaceRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReelRecord {
    pub id: String,
    pub url: String,
    pub shortcode: Option<String>,
    pub creator: Option<String>,
    pub caption: Option<String>,
    pub posted_at: Option<String>,
    pub media_path: Option<String>,
    pub audio_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub duration_sec: Option<f64>,
    pub media_source: Option<String>,
    pub status: ProcessingStatus,
    pub status_detail: Option<String>,
    pub failure_count: i64,
    pub classification: Classification,
    pub travel_confidence: f64,
    pub transcript: Vec<TranscriptSegment>,
    pub transcript_locale: Option<String>,
    pub content_hash: Option<String>,
    pub ai_model: Option<String>,
    pub ai_prompt_version: i64,
    pub ai_input_tokens: i64,
    pub ai_output_tokens: i64,
    pub ai_cost: f64,
    pub created_at: String,
    pub processed_at: Option<String>,
    pub place_count: i64,
    pub open_review_count: i64,
    /// {"caption": {"status": "done", "detail": "…"}, …} — see `pipeline::reels::STAGES`.
    pub stages: serde_json::Value,
    pub transcript_locale_override: Option<String>,
    pub transcript_confidence: Option<f64>,
    pub places_extracted: i64,
    pub places_auto_resolved: i64,
}

pub const REEL_COLUMNS: &str = "rl.id, rl.url, rl.shortcode, rl.creator, rl.caption, rl.posted_at, rl.media_path, rl.audio_path, \
    rl.thumbnail_path, rl.duration_sec, rl.media_source, rl.status, rl.status_detail, rl.failure_count, rl.classification, \
    rl.travel_confidence, rl.transcript, rl.transcript_locale, rl.content_hash, rl.ai_model, rl.ai_prompt_version, \
    rl.ai_input_tokens, rl.ai_output_tokens, rl.ai_cost, rl.created_at, rl.processed_at, \
    (SELECT COUNT(*) FROM place_reels pr WHERE pr.reel_id = rl.id), \
    (SELECT COUNT(*) FROM review_items r WHERE r.reel_id = rl.id AND r.is_resolved = 0), \
    rl.stages, rl.transcript_locale_override, rl.transcript_confidence, rl.places_extracted, rl.places_auto_resolved";

impl ReelRecord {
    pub fn from_row(r: &Row) -> rusqlite::Result<Self> {
        let transcript: String = r.get(16)?;
        Ok(Self {
            id: r.get(0)?,
            url: r.get(1)?,
            shortcode: r.get(2)?,
            creator: r.get(3)?,
            caption: r.get(4)?,
            posted_at: r.get(5)?,
            media_path: r.get(6)?,
            audio_path: r.get(7)?,
            thumbnail_path: r.get(8)?,
            duration_sec: r.get(9)?,
            media_source: r.get(10)?,
            status: r.get(11)?,
            status_detail: r.get(12)?,
            failure_count: r.get(13)?,
            classification: r.get(14)?,
            travel_confidence: r.get(15)?,
            transcript: serde_json::from_str(&transcript).unwrap_or_default(),
            transcript_locale: r.get(17)?,
            content_hash: r.get(18)?,
            ai_model: r.get(19)?,
            ai_prompt_version: r.get(20)?,
            ai_input_tokens: r.get(21)?,
            ai_output_tokens: r.get(22)?,
            ai_cost: r.get(23)?,
            created_at: r.get(24)?,
            processed_at: r.get(25)?,
            place_count: r.get(26)?,
            open_review_count: r.get(27)?,
            stages: serde_json::from_str(&r.get::<_, String>(28)?).unwrap_or_default(),
            transcript_locale_override: r.get(29)?,
            transcript_confidence: r.get(30)?,
            places_extracted: r.get(31)?,
            places_auto_resolved: r.get(32)?,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyframeRecord {
    pub id: String,
    pub time_sec: f64,
    pub image_path: String,
    pub ocr_text: String,
    pub is_cover: bool,
}
