//! Entity-level queries.

use std::collections::HashMap;

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;

use super::records::*;
use super::reels_repo::FactProvenance;
use super::{new_id, now, Database};
use crate::config::{AI_PROMPT_VERSION, OCR_VERSION, PROCESSING_VERSION};
use crate::models::*;
use crate::services::ai::types::{AiUsage, ExtractedPlace};
use crate::services::native::{AssetInfo, OcrBlockData};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlaceFilter {
    pub search: Option<String>,
    pub category: Option<String>,
    pub status: Option<String>,
    pub country_code: Option<String>,
    pub city: Option<String>,
    pub verified_only: bool,
    /// "recent" | "name" | "sources"
    pub sort: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScreenshotFilter {
    /// all | processed | needsReview | multiplePlaces | noPlace | lowConfidence | ignored | notTravel | failed | pending | everything
    pub view: Option<String>,
    pub search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl Database {
    // MARK: Screenshots

    /// Inserts newly discovered assets (existing ones are untouched) and queues them.
    pub fn upsert_discovered(&self, assets: &[AssetInfo]) -> Result<usize> {
        self.transaction(|tx| {
            let mut inserted = 0;
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO screenshots(id, photos_id, creation_date, width, height, discovered_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for a in assets {
                inserted += insert.execute(params![new_id(), a.id, a.creation_date, a.width, a.height, now()])?;
            }
            Ok(inserted)
        })
    }

    pub fn screenshot(&self, id: &str) -> Result<Option<ScreenshotRecord>> {
        self.with(|c| {
            c.query_row(&format!("SELECT {SCREENSHOT_COLUMNS} FROM screenshots s WHERE s.id = ?1"), [id], ScreenshotRecord::from_row)
                .optional()
        })
    }

    /// Screenshots the queue should pick up (restart recovery included), newest first.
    pub fn screenshot_ids_needing_processing(&self, limit: Option<i64>) -> Result<Vec<String>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id FROM screenshots WHERE status NOT IN ('complete','notTravel','needsReview','ignored') \
                 ORDER BY CASE status WHEN 'failed' THEN 1 ELSE 0 END, creation_date DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map([limit.unwrap_or(-1)], |r| r.get(0))?;
            rows.collect()
        })
    }

    pub fn set_status(&self, id: &str, status: ProcessingStatus, detail: Option<&str>) -> Result<()> {
        self.with(|c| c.execute("UPDATE screenshots SET status = ?2, status_detail = ?3 WHERE id = ?1", params![id, status, detail]))?;
        Ok(())
    }

    /// Stores every OCR block (text, confidence, geometry) — never just the concatenated text.
    pub fn save_ocr(&self, id: &str, blocks: &[OcrBlockData], image_path: &str, thumbnail_path: Option<&str>) -> Result<Vec<OcrBlockRecord>> {
        let ordered = crate::pipeline::reading_order(blocks);
        let full_text = ordered.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n");
        let hash = crate::text::stable_hash(&full_text);
        self.transaction(|tx| {
            tx.execute("DELETE FROM ocr_blocks WHERE screenshot_id = ?1", [id])?;
            let mut out = Vec::with_capacity(ordered.len());
            {
                let mut stmt = tx.prepare(
                    "INSERT INTO ocr_blocks(id, screenshot_id, idx, text, confidence, x, y, width, height, language) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                )?;
                for (i, b) in ordered.iter().enumerate() {
                    let block_id = new_id();
                    stmt.execute(params![block_id, id, i as i64, b.text, b.confidence, b.x, b.y, b.width, b.height, b.language])?;
                    out.push(OcrBlockRecord {
                        id: block_id, idx: i as i64, text: b.text.clone(), confidence: b.confidence,
                        x: b.x, y: b.y, width: b.width, height: b.height, language: b.language.clone(),
                    });
                }
            }
            tx.execute(
                "UPDATE screenshots SET ocr_full_text = ?2, ocr_hash = ?3, ocr_version = ?4, image_path = ?5, \
                 thumbnail_path = COALESCE(?6, thumbnail_path), status = 'ocrComplete' WHERE id = ?1",
                params![id, full_text, hash, OCR_VERSION, image_path, thumbnail_path],
            )?;
            Ok(out)
        })
    }

    pub fn ocr_blocks(&self, screenshot_id: &str) -> Result<Vec<OcrBlockRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, idx, text, confidence, x, y, width, height, language FROM ocr_blocks WHERE screenshot_id = ?1 ORDER BY idx",
            )?;
            let rows = stmt.query_map([screenshot_id], |r| {
                Ok(OcrBlockRecord {
                    id: r.get(0)?, idx: r.get(1)?, text: r.get(2)?, confidence: r.get(3)?, x: r.get(4)?,
                    y: r.get(5)?, width: r.get(6)?, height: r.get(7)?, language: r.get(8)?,
                })
            })?;
            rows.collect()
        })
    }

    pub fn set_image_paths(&self, id: &str, image_path: &str, thumbnail_path: Option<&str>) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE screenshots SET image_path = ?2, thumbnail_path = COALESCE(?3, thumbnail_path) WHERE id = ?1",
            params![id, image_path, thumbnail_path],
        ))?;
        Ok(())
    }

    pub fn set_local_analysis(&self, id: &str, score: f64, source: SourceType, creator: Option<&str>) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE screenshots SET local_travel_score = ?2, source_type = ?3, creator = COALESCE(?4, creator) WHERE id = ?1",
            params![id, score, source, creator],
        ))?;
        Ok(())
    }

    pub fn set_classification(&self, id: &str, classification: Classification, confidence: Option<f64>, escalation: i64) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE screenshots SET classification = ?2, travel_confidence = COALESCE(?3, travel_confidence), \
             escalation_level = MAX(escalation_level, ?4) WHERE id = ?1",
            params![id, classification, confidence, escalation],
        ))?;
        Ok(())
    }

    pub fn set_ai_details(&self, id: &str, source: Option<SourceType>, creator: Option<&str>, usage: &AiUsage) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE screenshots SET source_type = COALESCE(?2, source_type), creator = COALESCE(?3, creator), ai_model = ?4, \
             ai_thinking = ai_thinking OR ?5, ai_image_used = ai_image_used OR ?6, ai_input_tokens = ai_input_tokens + ?7, \
             ai_output_tokens = ai_output_tokens + ?8, ai_latency_ms = ai_latency_ms + ?9, ai_retry_count = ai_retry_count + ?10, \
             ai_cost = ai_cost + ?11 WHERE id = ?1",
            params![id, source, creator, usage.model, usage.thinking, usage.image_used, usage.input_tokens as i64,
                    usage.output_tokens as i64, usage.latency_ms as i64, usage.retries as i64, usage.estimated_cost],
        ))?;
        Ok(())
    }

    pub fn set_user_classification(&self, id: &str, value: Option<Classification>) -> Result<()> {
        self.with(|c| c.execute("UPDATE screenshots SET user_classification = ?2 WHERE id = ?1", params![id, value]))?;
        Ok(())
    }

    /// Terminal state reached: stamp versions so outdated results can be found later.
    pub fn finish_screenshot(&self, id: &str, status: ProcessingStatus, detail: Option<&str>) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE screenshots SET status = ?2, status_detail = ?3, processed_at = ?4, processing_version = ?5, \
             ai_prompt_version = CASE WHEN escalation_level >= 2 THEN ?6 ELSE ai_prompt_version END WHERE id = ?1",
            params![id, status, detail, now(), PROCESSING_VERSION, AI_PROMPT_VERSION],
        ))?;
        Ok(())
    }

    pub fn record_failure(&self, id: &str, message: &str) -> Result<i64> {
        self.with(|c| {
            c.execute(
                "UPDATE screenshots SET status = 'failed', status_detail = ?2, failure_count = failure_count + 1 WHERE id = ?1",
                params![id, message],
            )?;
            c.query_row("SELECT failure_count FROM screenshots WHERE id = ?1", [id], |r| r.get(0))
        })
    }

    pub fn list_screenshots(&self, f: &ScreenshotFilter) -> Result<Vec<ScreenshotRecord>> {
        let condition = match f.view.as_deref().unwrap_or("all") {
            "processed" => "s.status = 'complete'",
            "needsReview" => "s.status = 'needsReview'",
            "multiplePlaces" => "(SELECT COUNT(*) FROM place_screenshots ps WHERE ps.screenshot_id = s.id) > 1",
            "noPlace" => "(s.classification = 'travel' OR s.user_classification = 'travel') AND NOT EXISTS (SELECT 1 FROM place_screenshots ps WHERE ps.screenshot_id = s.id)",
            "lowConfidence" => "s.classification IN ('travel','uncertain') AND s.travel_confidence < 0.8",
            "ignored" => "s.status = 'ignored'",
            "notTravel" => "s.status = 'notTravel'",
            "failed" => "s.status = 'failed'",
            "pending" => "s.status NOT IN ('complete','notTravel','needsReview','ignored','failed')",
            "everything" => "1 = 1",
            _ => "(s.classification IN ('travel','uncertain') OR s.user_classification = 'travel') AND s.status != 'ignored'",
        };
        let search = f.search.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| format!("%{s}%"));
        let sql = format!(
            "SELECT {SCREENSHOT_COLUMNS} FROM screenshots s WHERE {condition} \
             AND (?1 IS NULL OR s.ocr_full_text LIKE ?1 OR s.creator LIKE ?1 OR s.source_type LIKE ?1) \
             ORDER BY s.creation_date DESC LIMIT ?2 OFFSET ?3"
        );
        self.with(|c| {
            let mut stmt = c.prepare(&sql)?;
            let rows = stmt.query_map(params![search, f.limit.unwrap_or(500), f.offset.unwrap_or(0)], ScreenshotRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn screenshot_counts(&self) -> Result<HashMap<String, i64>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT status, COUNT(*) FROM screenshots GROUP BY status")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            rows.collect()
        })
    }

    /// Clears what automatic processing derived from a screenshot. User-verified links, user
    /// facts and user crops are kept: corrections always survive re-processing.
    pub fn clear_derived(&self, screenshot_id: &str, keep_ocr: bool) -> Result<()> {
        self.transaction(|tx| {
            tx.execute("DELETE FROM place_screenshots WHERE screenshot_id = ?1 AND is_user_verified = 0", [screenshot_id])?;
            tx.execute("DELETE FROM travel_facts WHERE screenshot_id = ?1 AND origin != 'user'", [screenshot_id])?;
            tx.execute("DELETE FROM place_images WHERE screenshot_id = ?1 AND origin != 'user'", [screenshot_id])?;
            tx.execute("DELETE FROM review_items WHERE screenshot_id = ?1", [screenshot_id])?;
            // Places left with no evidence and no user edits disappear with it.
            tx.execute(
                "DELETE FROM places WHERE is_user_verified = 0 AND notes = '' AND personal_status = 'wantToVisit' \
                 AND NOT EXISTS (SELECT 1 FROM place_screenshots ps WHERE ps.place_id = places.id) \
                 AND NOT EXISTS (SELECT 1 FROM place_reels pr WHERE pr.place_id = places.id) \
                 AND NOT EXISTS (SELECT 1 FROM trip_places t WHERE t.place_id = places.id)",
                [],
            )?;
            tx.execute(
                &format!(
                    "UPDATE screenshots SET status = '{}', status_detail = NULL, classification = 'unknown', escalation_level = 0, \
                     ai_model = NULL, ai_thinking = 0, ai_image_used = 0, ai_input_tokens = 0, ai_output_tokens = 0, ai_latency_ms = 0, \
                     ai_retry_count = 0, ai_cost = 0, places_extracted = 0, places_auto_resolved = 0, pipeline_ms = 0 WHERE id = ?1",
                    if keep_ocr { "ocrComplete" } else { "discovered" }
                ),
                [screenshot_id],
            )?;
            Ok(())
        })
    }

    /// Marks a set of screenshots for re-processing (keeps OCR). Returns the number affected.
    pub fn mark_for_reprocess(&self, scope: &str, ids: &[String]) -> Result<Vec<String>> {
        let targets: Vec<String> = match scope {
            "selected" => ids.to_vec(),
            _ => {
                let condition = match scope {
                    "failed" => "status = 'failed'",
                    "needsReview" => "status = 'needsReview'",
                    "outdated" => "escalation_level >= 2 AND ai_prompt_version < ?1",
                    "all" => "status != 'ignored'",
                    _ => "0",
                };
                self.with(|c| {
                    let mut stmt = c.prepare(&format!("SELECT id FROM screenshots WHERE {condition} AND ?1 = ?1"))?;
                    let rows = stmt.query_map([AI_PROMPT_VERSION], |r| r.get(0))?;
                    rows.collect()
                })?
            }
        };
        for id in &targets {
            let keep_ocr = self.screenshot(id)?.is_some_and(|s| s.ocr_version >= OCR_VERSION);
            self.clear_derived(id, keep_ocr)?;
        }
        Ok(targets)
    }

    // MARK: AI cache of extraction per screenshot

    pub fn cached_extraction_for_screenshot(&self, screenshot_id: &str) -> Result<Option<String>> {
        self.with(|c| c.query_row(
            "SELECT result_json FROM ai_cache WHERE ocr_hash = ?1 AND kind = 'extraction' ORDER BY prompt_version DESC, created_at DESC LIMIT 1",
            [format!("shot:{screenshot_id}")],
            |r| r.get(0),
        ).optional())
    }

    // MARK: Places

    pub fn insert_place(&self, cand: &PlaceCandidate, category: PlaceCategory, verification: Verification, origin: DataOrigin, alt_names: &[String]) -> Result<PlaceRecord> {
        let id = new_id();
        let ts = now();
        self.with(|c| c.execute(
            "INSERT INTO places(id, canonical_name, alternative_names, map_identifier, latitude, longitude, address, city, region, \
             country, country_code, category, verification, origin, is_user_verified, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?16)",
            params![id, cand.name, serde_json::to_string(alt_names).unwrap(), cand.map_identifier, cand.latitude, cand.longitude,
                    cand.address, cand.city, cand.region, cand.country, cand.country_code, category, verification, origin,
                    origin == DataOrigin::User, ts],
        ))?;
        Ok(self.place(&id)?.expect("inserted place"))
    }

    pub fn place(&self, id: &str) -> Result<Option<PlaceRecord>> {
        self.with(|c| c.query_row(&format!("SELECT {PLACE_COLUMNS} FROM places p WHERE p.id = ?1"), [id], PlaceRecord::from_row).optional())
    }

    pub fn place_by_map_identifier(&self, map_id: &str) -> Result<Option<PlaceRecord>> {
        self.with(|c| c.query_row(&format!("SELECT {PLACE_COLUMNS} FROM places p WHERE p.map_identifier = ?1 LIMIT 1"), [map_id], PlaceRecord::from_row).optional())
    }

    /// Places inside a lat/lon box (candidate set for proximity-based de-duplication).
    pub fn places_near(&self, lat: f64, lon: f64, delta: f64) -> Result<Vec<PlaceRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {PLACE_COLUMNS} FROM places p WHERE p.latitude BETWEEN ?1 AND ?2 AND p.longitude BETWEEN ?3 AND ?4"
            ))?;
            let rows = stmt.query_map(params![lat - delta, lat + delta, lon - delta, lon + delta], PlaceRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn list_places(&self, f: &PlaceFilter) -> Result<Vec<PlaceRecord>> {
        let order = match f.sort.as_deref() {
            Some("name") => "p.canonical_name COLLATE NOCASE",
            Some("sources") => "(SELECT COUNT(*) FROM place_screenshots ps WHERE ps.place_id = p.id) DESC",
            _ => "p.created_at DESC",
        };
        let mut places: Vec<PlaceRecord> = self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {PLACE_COLUMNS} FROM places p WHERE (?1 IS NULL OR p.category = ?1) AND (?2 IS NULL OR p.personal_status = ?2) \
                 AND (?3 IS NULL OR p.country_code = ?3) AND (?4 IS NULL OR p.city = ?4) \
                 AND (?5 = 0 OR p.verification != 'needsReview') ORDER BY {order}"
            ))?;
            let rows = stmt.query_map(
                params![f.category, f.status, f.country_code, f.city, f.verified_only],
                PlaceRecord::from_row,
            )?;
            rows.collect()
        })?;

        if let Some(q) = f.search.as_deref().map(crate::text::normalize).filter(|q| !q.is_empty()) {
            // V1 text search: name, location, category, fact text, creator and source app.
            let evidence: HashMap<String, String> = self.with(|c| {
                let mut stmt = c.prepare(
                    "SELECT p.id, COALESCE((SELECT group_concat(text, ' ') FROM travel_facts f WHERE f.place_id = p.id), '') || ' ' || \
                     COALESCE((SELECT group_concat(COALESCE(s.creator,'') || ' ' || s.source_type, ' ') FROM place_screenshots ps \
                               JOIN screenshots s ON s.id = ps.screenshot_id WHERE ps.place_id = p.id), '') FROM places p",
                )?;
                let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                rows.collect()
            })?;
            places.retain(|p| {
                let hay = crate::text::normalize(&format!(
                    "{} {} {} {} {} {} {}",
                    p.all_names().join(" "), p.city.clone().unwrap_or_default(), p.region.clone().unwrap_or_default(),
                    p.country.clone().unwrap_or_default(), p.category.as_str(), p.notes,
                    evidence.get(&p.id).cloned().unwrap_or_default()
                ));
                q.split(' ').all(|term| hay.contains(term))
            });
        }
        Ok(places)
    }

    pub fn update_place_location(&self, id: &str, cand: &PlaceCandidate, verification: Verification) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE places SET canonical_name = ?2, latitude = ?3, longitude = ?4, address = ?5, city = ?6, region = ?7, country = ?8, \
             country_code = ?9, map_identifier = ?10, verification = ?11, updated_at = ?12 WHERE id = ?1",
            params![id, cand.name, cand.latitude, cand.longitude, cand.address, cand.city, cand.region, cand.country,
                    cand.country_code, cand.map_identifier, verification, now()],
        ))?;
        Ok(())
    }

    pub fn update_place_field(&self, id: &str, field: &str, value: Option<&str>) -> Result<()> {
        let column = match field {
            "canonicalName" => "canonical_name",
            "category" => "category",
            "personalStatus" => "personal_status",
            "notes" => "notes",
            "heroImageId" => "hero_image_id",
            "verification" => "verification",
            _ => anyhow::bail!("unknown place field {field}"),
        };
        self.with(|c| c.execute(&format!("UPDATE places SET {column} = ?2, updated_at = ?3 WHERE id = ?1"), params![id, value, now()]))?;
        Ok(())
    }

    pub fn set_place_user_verified(&self, id: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE places SET is_user_verified = 1, verification = 'userVerified', updated_at = ?2 WHERE id = ?1", params![id, now()]))?;
        Ok(())
    }

    pub fn set_place_verification(&self, id: &str, v: Verification) -> Result<()> {
        self.with(|c| c.execute("UPDATE places SET verification = ?2 WHERE id = ?1", params![id, v]))?;
        Ok(())
    }

    pub fn set_alternative_names(&self, id: &str, names: &[String]) -> Result<()> {
        self.with(|c| c.execute("UPDATE places SET alternative_names = ?2 WHERE id = ?1", params![id, serde_json::to_string(names).unwrap()]))?;
        Ok(())
    }

    pub fn set_place_summary(&self, id: &str, text: &str, fact_ids: &[String]) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE places SET summary_text = ?2, summary_fact_ids = ?3, summary_generated_at = ?4 WHERE id = ?1",
            params![id, text, serde_json::to_string(fact_ids).unwrap(), now()],
        ))?;
        Ok(())
    }

    pub fn delete_place(&self, id: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM places WHERE id = ?1", [id]))?;
        Ok(())
    }

    // MARK: Links

    pub fn link(&self, place_id: &str, screenshot_id: &str, extracted_name: &str, confidence: f64, origin: DataOrigin) -> Result<bool> {
        let n = self.with(|c| c.execute(
            "INSERT INTO place_screenshots(place_id, screenshot_id, extracted_name, confidence, origin, is_user_verified, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(place_id, screenshot_id) DO UPDATE SET \
             is_user_verified = MAX(is_user_verified, excluded.is_user_verified), origin = CASE WHEN excluded.is_user_verified = 1 THEN 'user' ELSE origin END",
            params![place_id, screenshot_id, extracted_name, confidence, origin, origin == DataOrigin::User, now()],
        ))?;
        Ok(n > 0)
    }

    pub fn unlink(&self, place_id: &str, screenshot_id: &str) -> Result<()> {
        self.transaction(|tx| {
            tx.execute("DELETE FROM place_screenshots WHERE place_id = ?1 AND screenshot_id = ?2", params![place_id, screenshot_id])?;
            tx.execute("DELETE FROM travel_facts WHERE place_id = ?1 AND screenshot_id = ?2", params![place_id, screenshot_id])?;
            tx.execute("DELETE FROM place_images WHERE place_id = ?1 AND screenshot_id = ?2", params![place_id, screenshot_id])?;
            Ok(())
        })
    }

    pub fn links_for_place(&self, place_id: &str) -> Result<Vec<LinkRecord>> {
        self.links_where("place_id", place_id)
    }

    pub fn links_for_screenshot(&self, screenshot_id: &str) -> Result<Vec<LinkRecord>> {
        self.links_where("screenshot_id", screenshot_id)
    }

    fn links_where(&self, column: &str, value: &str) -> Result<Vec<LinkRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT place_id, screenshot_id, extracted_name, confidence, origin, is_user_verified, created_at FROM place_screenshots WHERE {column} = ?1"
            ))?;
            let rows = stmt.query_map([value], |r| Ok(LinkRecord {
                place_id: r.get(0)?, screenshot_id: r.get(1)?, extracted_name: r.get(2)?, confidence: r.get(3)?,
                origin: r.get(4)?, is_user_verified: r.get(5)?, created_at: r.get(6)?,
            }))?;
            rows.collect()
        })
    }

    pub fn screenshots_for_place(&self, place_id: &str) -> Result<Vec<ScreenshotRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {SCREENSHOT_COLUMNS} FROM screenshots s JOIN place_screenshots ps ON ps.screenshot_id = s.id \
                 WHERE ps.place_id = ?1 ORDER BY s.creation_date DESC"
            ))?;
            let rows = stmt.query_map([place_id], ScreenshotRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn places_for_screenshot(&self, screenshot_id: &str) -> Result<Vec<PlaceRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {PLACE_COLUMNS} FROM places p JOIN place_screenshots ps ON ps.place_id = p.id WHERE ps.screenshot_id = ?1"
            ))?;
            let rows = stmt.query_map([screenshot_id], PlaceRecord::from_row)?;
            rows.collect()
        })
    }

    // MARK: Facts

    pub fn insert_fact(&self, place_id: &str, kind: TravelFactType, text: &str, quote: Option<&str>, confidence: f64,
                       source: &FactProvenance, origin: DataOrigin) -> Result<String> {
        let id = new_id();
        self.with(|c| c.execute(
            "INSERT INTO travel_facts(id, place_id, screenshot_id, reel_id, source_kind, source_time_sec, type, text, source_quote, \
             confidence, source_block_ids, valid_from, origin, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![id, place_id, source.screenshot_id, source.reel_id, source.kind, source.time_sec, kind, text, quote, confidence,
                    serde_json::to_string(&source.block_ids).unwrap(), source.valid_from, origin, now()],
        ))?;
        Ok(id)
    }

    pub fn facts_for_place(&self, place_id: &str) -> Result<Vec<FactRecord>> {
        self.facts_where("f.place_id", place_id)
    }

    pub fn facts_for_screenshot(&self, screenshot_id: &str) -> Result<Vec<FactRecord>> {
        self.facts_where("f.screenshot_id", screenshot_id)
    }

    fn facts_where(&self, column: &str, value: &str) -> Result<Vec<FactRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {FACT_COLUMNS} FROM travel_facts f LEFT JOIN screenshots s ON s.id = f.screenshot_id LEFT JOIN reels rl ON rl.id = f.reel_id \
                 WHERE {column} = ?1 ORDER BY f.type, COALESCE(f.valid_from, f.created_at) DESC"
            ))?;
            let rows = stmt.query_map([value], FactRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn delete_fact(&self, id: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM travel_facts WHERE id = ?1", [id]))?;
        Ok(())
    }

    // MARK: Images

    #[allow(clippy::too_many_arguments)]
    pub fn insert_image(&self, place_id: &str, screenshot_id: Option<&str>, crop: Rect, image_path: &str, quality: f64, region: RegionType,
                        region_confidence: f64, feature_print: Option<&[f32]>, accepted: bool, origin: DataOrigin) -> Result<String> {
        let id = new_id();
        let blob: Option<Vec<u8>> = feature_print.map(|v| v.iter().flat_map(|f| f.to_le_bytes()).collect());
        self.with(|c| c.execute(
            "INSERT INTO place_images(id, place_id, screenshot_id, crop_x, crop_y, crop_width, crop_height, image_path, quality_score, \
             region_type, region_confidence, feature_print, is_accepted, origin, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![id, place_id, screenshot_id, crop.x, crop.y, crop.width, crop.height, image_path, quality, region,
                    region_confidence, blob, accepted, origin, now()],
        ))?;
        Ok(id)
    }

    pub fn images_for_place(&self, place_id: &str) -> Result<Vec<PlaceImageRecord>> {
        self.images_where("place_id", place_id)
    }

    pub fn images_for_screenshot(&self, screenshot_id: &str) -> Result<Vec<PlaceImageRecord>> {
        self.images_where("screenshot_id", screenshot_id)
    }

    fn images_where(&self, column: &str, value: &str) -> Result<Vec<PlaceImageRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!("SELECT {IMAGE_COLUMNS} FROM place_images WHERE {column} = ?1 ORDER BY quality_score DESC"))?;
            let rows = stmt.query_map([value], PlaceImageRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn image(&self, id: &str) -> Result<Option<PlaceImageRecord>> {
        self.with(|c| c.query_row(&format!("SELECT {IMAGE_COLUMNS} FROM place_images WHERE id = ?1"), [id], PlaceImageRecord::from_row).optional())
    }

    pub fn set_image_accepted(&self, id: &str, accepted: bool) -> Result<()> {
        self.with(|c| c.execute("UPDATE place_images SET is_accepted = ?2 WHERE id = ?1", params![id, accepted]))?;
        Ok(())
    }

    pub fn add_duplicate_source(&self, image_id: &str, screenshot_id: &str) -> Result<()> {
        if let Some(img) = self.image(image_id)? {
            let mut ids = img.duplicate_source_ids;
            if !ids.iter().any(|i| i == screenshot_id) && img.screenshot_id.as_deref() != Some(screenshot_id) {
                ids.push(screenshot_id.to_string());
            }
            self.with(|c| c.execute("UPDATE place_images SET duplicate_source_ids = ?2 WHERE id = ?1", params![image_id, serde_json::to_string(&ids).unwrap()]))?;
        }
        Ok(())
    }

    pub fn delete_image(&self, id: &str) -> Result<()> {
        self.with(|c| {
            c.execute("UPDATE places SET hero_image_id = NULL WHERE hero_image_id = ?1", [id])?;
            c.execute("DELETE FROM place_images WHERE id = ?1", [id])
        })?;
        Ok(())
    }

    // MARK: Review items

    #[allow(clippy::too_many_arguments)]
    pub fn insert_review(&self, kind: ReviewKind, screenshot_id: Option<&str>, message: &str, extracted: Option<&ExtractedPlace>,
                         candidates: &[PlaceCandidate], place_a: Option<&str>, place_b: Option<&str>, image_id: Option<&str>) -> Result<String> {
        let id = new_id();
        self.with(|c| c.execute(
            "INSERT INTO review_items(id, kind, screenshot_id, message, extracted_place, candidates, place_a_id, place_b_id, image_id, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![id, kind, screenshot_id, message, extracted.map(|e| serde_json::to_string(e).unwrap()),
                    serde_json::to_string(candidates).unwrap(), place_a, place_b, image_id, now()],
        ))?;
        Ok(id)
    }

    pub fn open_reviews(&self) -> Result<Vec<ReviewRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {REVIEW_COLUMNS} FROM review_items r LEFT JOIN screenshots s ON s.id = r.screenshot_id LEFT JOIN reels rl ON rl.id = r.reel_id \
                 WHERE r.is_resolved = 0 ORDER BY r.created_at DESC"
            ))?;
            let rows = stmt.query_map([], ReviewRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn reviews_for_screenshot(&self, screenshot_id: &str) -> Result<Vec<ReviewRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {REVIEW_COLUMNS} FROM review_items r LEFT JOIN screenshots s ON s.id = r.screenshot_id LEFT JOIN reels rl ON rl.id = r.reel_id WHERE r.screenshot_id = ?1"
            ))?;
            let rows = stmt.query_map([screenshot_id], ReviewRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn review(&self, id: &str) -> Result<Option<ReviewRecord>> {
        self.with(|c| c.query_row(
            &format!("SELECT {REVIEW_COLUMNS} FROM review_items r LEFT JOIN screenshots s ON s.id = r.screenshot_id LEFT JOIN reels rl ON rl.id = r.reel_id WHERE r.id = ?1"),
            [id], ReviewRecord::from_row,
        ).optional())
    }

    pub fn resolve_review(&self, id: &str, resolution: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE review_items SET is_resolved = 1, resolution = ?2 WHERE id = ?1", params![id, resolution]))?;
        Ok(())
    }

    pub fn open_review_count(&self) -> Result<i64> {
        self.with(|c| c.query_row("SELECT COUNT(*) FROM review_items WHERE is_resolved = 0", [], |r| r.get(0)))
    }

    // MARK: Trips

    pub fn list_trips(&self) -> Result<Vec<TripRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT t.id, t.name, t.start_date, t.end_date, t.notes, t.created_at, \
                 (SELECT COUNT(*) FROM trip_places tp WHERE tp.trip_id = t.id) FROM trips t ORDER BY t.created_at DESC",
            )?;
            let rows = stmt.query_map([], |r| Ok(TripRecord {
                id: r.get(0)?, name: r.get(1)?, start_date: r.get(2)?, end_date: r.get(3)?, notes: r.get(4)?,
                created_at: r.get(5)?, place_count: r.get(6)?,
            }))?;
            rows.collect()
        })
    }

    pub fn create_trip(&self, name: &str) -> Result<String> {
        let id = new_id();
        self.with(|c| c.execute("INSERT INTO trips(id, name, created_at) VALUES (?1, ?2, ?3)", params![id, name, now()]))?;
        Ok(id)
    }

    pub fn update_trip(&self, id: &str, name: &str, start: Option<&str>, end: Option<&str>, notes: &str) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE trips SET name = ?2, start_date = ?3, end_date = ?4, notes = ?5 WHERE id = ?1",
            params![id, name, start, end, notes],
        ))?;
        Ok(())
    }

    pub fn delete_trip(&self, id: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM trips WHERE id = ?1", [id]))?;
        Ok(())
    }

    pub fn trip_entries(&self, trip_id: &str) -> Result<Vec<TripEntryRecord>> {
        let entries: Vec<(String, String, i64, Option<i64>, String)> = self.with(|c| {
            let mut stmt = c.prepare("SELECT id, trip_id, position, day, place_id FROM trip_places WHERE trip_id = ?1 ORDER BY COALESCE(day, 9999), position")?;
            let rows = stmt.query_map([trip_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?;
            rows.collect()
        })?;
        let mut out = Vec::new();
        for (id, trip_id, position, day, place_id) in entries {
            if let Some(place) = self.place(&place_id)? {
                out.push(TripEntryRecord { id, trip_id, position, day, place });
            }
        }
        Ok(out)
    }

    pub fn add_to_trip(&self, trip_id: &str, place_id: &str) -> Result<()> {
        self.with(|c| c.execute(
            "INSERT OR IGNORE INTO trip_places(id, trip_id, place_id, position) \
             VALUES (?1, ?2, ?3, (SELECT COALESCE(MAX(position), -1) + 1 FROM trip_places WHERE trip_id = ?2))",
            params![new_id(), trip_id, place_id],
        ))?;
        Ok(())
    }

    pub fn update_trip_entry(&self, entry_id: &str, day: Option<i64>, position: Option<i64>) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE trip_places SET day = ?2, position = COALESCE(?3, position) WHERE id = ?1",
            params![entry_id, day, position],
        ))?;
        Ok(())
    }

    pub fn remove_trip_entry(&self, entry_id: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM trip_places WHERE id = ?1", [entry_id]))?;
        Ok(())
    }

    pub fn trips_for_place(&self, place_id: &str) -> Result<Vec<TripRecord>> {
        Ok(self.list_trips()?.into_iter().filter(|t| {
            self.with(|c| c.query_row("SELECT COUNT(*) FROM trip_places WHERE trip_id = ?1 AND place_id = ?2", params![t.id, place_id], |r| r.get::<_, i64>(0)))
                .unwrap_or(0) > 0
        }).collect())
    }
}
