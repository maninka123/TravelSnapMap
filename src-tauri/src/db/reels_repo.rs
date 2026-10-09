//! Reel storage and cross-source provenance.

use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use super::records::*;
use super::{new_id, now, Database};
use crate::models::*;

/// Where a fact came from: a screenshot (with OCR blocks) or a moment in a Reel.
#[derive(Debug, Clone, Default)]
pub struct FactProvenance<'a> {
    pub screenshot_id: Option<&'a str>,
    pub reel_id: Option<&'a str>,
    /// screenshot | audio | caption | keyframe | user
    pub kind: &'a str,
    pub time_sec: Option<f64>,
    pub block_ids: Vec<String>,
    pub valid_from: Option<&'a str>,
}

impl Database {
    /// Creates a Reel for a URL, or returns the existing one (same shortcode ⇒ same Reel).
    pub fn insert_reel(&self, url: &str, shortcode: Option<&str>) -> Result<(String, bool)> {
        if let Some(code) = shortcode {
            let existing: Option<String> = self.with(|c| c.query_row("SELECT id FROM reels WHERE shortcode = ?1", [code], |r| r.get(0)).optional())?;
            if let Some(id) = existing {
                return Ok((id, false));
            }
        }
        let id = new_id();
        self.with(|c| c.execute("INSERT INTO reels(id, url, shortcode, created_at) VALUES (?1, ?2, ?3, ?4)", params![id, url, shortcode, now()]))?;
        Ok((id, true))
    }

    pub fn reel(&self, id: &str) -> Result<Option<ReelRecord>> {
        self.with(|c| c.query_row(&format!("SELECT {REEL_COLUMNS} FROM reels rl WHERE rl.id = ?1"), [id], ReelRecord::from_row).optional())
    }

    pub fn list_reels(&self) -> Result<Vec<ReelRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!("SELECT {REEL_COLUMNS} FROM reels rl WHERE rl.status != 'ignored' ORDER BY rl.created_at DESC"))?;
            let rows = stmt.query_map([], ReelRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn reel_ids_needing_processing(&self) -> Result<Vec<String>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id FROM reels WHERE status NOT IN ('complete','notTravel','needsReview','ignored','needsMedia') ORDER BY created_at")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect()
        })
    }

    pub fn set_reel_status(&self, id: &str, status: ProcessingStatus, detail: Option<&str>) -> Result<()> {
        self.with(|c| c.execute("UPDATE reels SET status = ?2, status_detail = ?3 WHERE id = ?1", params![id, status, detail]))?;
        Ok(())
    }

    pub fn finish_reel(&self, id: &str, status: ProcessingStatus, detail: Option<&str>) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE reels SET status = ?2, status_detail = ?3, processed_at = ?4 WHERE id = ?1",
            params![id, status, detail, now()],
        ))?;
        Ok(())
    }

    pub fn record_reel_failure(&self, id: &str, message: &str) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE reels SET status = 'failed', status_detail = ?2, failure_count = failure_count + 1 WHERE id = ?1",
            params![id, message],
        ))?;
        Ok(())
    }

    pub fn set_reel_metadata(&self, id: &str, creator: Option<&str>, caption: Option<&str>, posted_at: Option<&str>, thumbnail: Option<&str>) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE reels SET creator = COALESCE(?2, creator), caption = COALESCE(?3, caption), posted_at = COALESCE(?4, posted_at), \
             thumbnail_path = COALESCE(?5, thumbnail_path) WHERE id = ?1",
            params![id, creator, caption, posted_at, thumbnail],
        ))?;
        Ok(())
    }

    pub fn set_reel_media(&self, id: &str, media_path: &str, source: &str, duration: Option<f64>) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE reels SET media_path = ?2, media_source = ?3, duration_sec = COALESCE(?4, duration_sec) WHERE id = ?1",
            params![id, media_path, source, duration],
        ))?;
        Ok(())
    }

    pub fn set_reel_transcript(&self, id: &str, audio_path: Option<&str>, segments: &[TranscriptSegment], locale: Option<&str>) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE reels SET audio_path = ?2, transcript = ?3, transcript_locale = ?4 WHERE id = ?1",
            params![id, audio_path, serde_json::to_string(segments).unwrap(), locale],
        ))?;
        Ok(())
    }

    pub fn set_reel_ai(&self, id: &str, classification: Classification, confidence: f64, hash: &str, model: &str, prompt_version: i64,
                       input_tokens: u64, output_tokens: u64, cost: f64) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE reels SET classification = ?2, travel_confidence = ?3, content_hash = ?4, ai_model = ?5, ai_prompt_version = ?6, \
             ai_input_tokens = ai_input_tokens + ?7, ai_output_tokens = ai_output_tokens + ?8, ai_cost = ai_cost + ?9 WHERE id = ?1",
            params![id, classification, confidence, hash, model, prompt_version, input_tokens as i64, output_tokens as i64, cost],
        ))?;
        Ok(())
    }

    pub fn replace_keyframes(&self, reel_id: &str, frames: &[(f64, String, String, String)]) -> Result<Vec<KeyframeRecord>> {
        self.transaction(|tx| {
            tx.execute("DELETE FROM reel_keyframes WHERE reel_id = ?1", [reel_id])?;
            let mut out = Vec::new();
            // The cover is the frame with the least on-screen text (usually the cleanest scene).
            let cover = frames.iter().enumerate().min_by_key(|(_, f)| f.2.chars().count()).map(|(i, _)| i);
            for (i, (time, path, text, blocks)) in frames.iter().enumerate() {
                let id = new_id();
                let is_cover = Some(i) == cover;
                tx.execute(
                    "INSERT INTO reel_keyframes(id, reel_id, time_sec, image_path, ocr_text, ocr_blocks, is_cover) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![id, reel_id, time, path, text, blocks, is_cover],
                )?;
                if is_cover {
                    tx.execute("UPDATE reels SET thumbnail_path = ?2 WHERE id = ?1", params![reel_id, path])?;
                }
                out.push(KeyframeRecord { id, time_sec: *time, image_path: path.clone(), ocr_text: text.clone(), is_cover });
            }
            Ok(out)
        })
    }

    pub fn keyframes(&self, reel_id: &str) -> Result<Vec<KeyframeRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT id, time_sec, image_path, ocr_text, is_cover FROM reel_keyframes WHERE reel_id = ?1 ORDER BY time_sec")?;
            let rows = stmt.query_map([reel_id], |r| Ok(KeyframeRecord {
                id: r.get(0)?, time_sec: r.get(1)?, image_path: r.get(2)?, ocr_text: r.get(3)?, is_cover: r.get(4)?,
            }))?;
            rows.collect()
        })
    }

    pub fn link_reel(&self, place_id: &str, reel_id: &str, extracted_name: &str, confidence: f64, origin: DataOrigin) -> Result<()> {
        self.with(|c| c.execute(
            "INSERT INTO place_reels(place_id, reel_id, extracted_name, confidence, origin, is_user_verified, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT(place_id, reel_id) DO UPDATE SET \
             is_user_verified = MAX(is_user_verified, excluded.is_user_verified)",
            params![place_id, reel_id, extracted_name, confidence, origin, origin == DataOrigin::User, now()],
        ))?;
        Ok(())
    }

    pub fn unlink_reel(&self, place_id: &str, reel_id: &str) -> Result<()> {
        self.transaction(|tx| {
            tx.execute("DELETE FROM place_reels WHERE place_id = ?1 AND reel_id = ?2", params![place_id, reel_id])?;
            tx.execute("DELETE FROM travel_facts WHERE place_id = ?1 AND reel_id = ?2", params![place_id, reel_id])?;
            tx.execute("DELETE FROM place_images WHERE place_id = ?1 AND reel_id = ?2", params![place_id, reel_id])?;
            Ok(())
        })
    }

    /// The name the place was found under in this Reel.
    pub fn reel_link_name(&self, place_id: &str, reel_id: &str) -> Result<Option<String>> {
        self.with(|c| c.query_row(
            "SELECT extracted_name FROM place_reels WHERE place_id = ?1 AND reel_id = ?2",
            params![place_id, reel_id],
            |r| r.get(0),
        ).optional())
    }

    /// (place id, extracted name) pairs the user confirmed for this Reel.
    pub fn user_reel_links(&self, reel_id: &str) -> Result<Vec<(String, String)>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT place_id, extracted_name FROM place_reels WHERE reel_id = ?1 AND is_user_verified = 1")?;
            let rows = stmt.query_map([reel_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        })
    }

    pub fn reels_for_place(&self, place_id: &str) -> Result<Vec<ReelRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {REEL_COLUMNS} FROM reels rl JOIN place_reels pr ON pr.reel_id = rl.id WHERE pr.place_id = ?1 ORDER BY rl.created_at DESC"
            ))?;
            let rows = stmt.query_map([place_id], ReelRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn places_for_reel(&self, reel_id: &str) -> Result<Vec<PlaceRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {PLACE_COLUMNS} FROM places p JOIN place_reels pr ON pr.place_id = p.id WHERE pr.reel_id = ?1"
            ))?;
            let rows = stmt.query_map([reel_id], PlaceRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn facts_for_reel(&self, reel_id: &str) -> Result<Vec<FactRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {FACT_COLUMNS} FROM travel_facts f LEFT JOIN screenshots s ON s.id = f.screenshot_id \
                 LEFT JOIN reels rl ON rl.id = f.reel_id WHERE f.reel_id = ?1 ORDER BY f.source_time_sec"
            ))?;
            let rows = stmt.query_map([reel_id], FactRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn reviews_for_reel(&self, reel_id: &str) -> Result<Vec<ReviewRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {REVIEW_COLUMNS} FROM review_items r LEFT JOIN screenshots s ON s.id = r.screenshot_id \
                 LEFT JOIN reels rl ON rl.id = r.reel_id WHERE r.reel_id = ?1"
            ))?;
            let rows = stmt.query_map([reel_id], ReviewRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn set_review_reel(&self, review_id: &str, reel_id: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE review_items SET reel_id = ?2 WHERE id = ?1", params![review_id, reel_id]))?;
        Ok(())
    }

    pub fn set_image_reel(&self, image_id: &str, reel_id: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE place_images SET reel_id = ?2, screenshot_id = NULL WHERE id = ?1", params![image_id, reel_id]))?;
        Ok(())
    }

    pub fn images_for_reel(&self, reel_id: &str) -> Result<Vec<PlaceImageRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!("SELECT {IMAGE_COLUMNS} FROM place_images WHERE reel_id = ?1 ORDER BY quality_score DESC"))?;
            let rows = stmt.query_map([reel_id], PlaceImageRecord::from_row)?;
            rows.collect()
        })
    }

    /// Clears automatic results for a Reel before re-processing (user-confirmed links are kept).
    pub fn clear_reel_derived(&self, reel_id: &str) -> Result<()> {
        self.transaction(|tx| {
            tx.execute("DELETE FROM place_reels WHERE reel_id = ?1 AND is_user_verified = 0", [reel_id])?;
            tx.execute("DELETE FROM travel_facts WHERE reel_id = ?1 AND origin != 'user'", [reel_id])?;
            tx.execute("DELETE FROM place_images WHERE reel_id = ?1 AND origin != 'user'", [reel_id])?;
            // Open questions are asked again if still relevant; answered ones stay in "Recently reviewed".
            tx.execute("DELETE FROM review_items WHERE reel_id = ?1 AND is_resolved = 0", [reel_id])?;
            tx.execute(
                "DELETE FROM places WHERE is_user_verified = 0 AND notes = '' AND personal_status = 'wantToVisit' \
                 AND NOT EXISTS (SELECT 1 FROM place_screenshots ps WHERE ps.place_id = places.id) \
                 AND NOT EXISTS (SELECT 1 FROM place_reels pr WHERE pr.place_id = places.id) \
                 AND NOT EXISTS (SELECT 1 FROM trip_places t WHERE t.place_id = places.id)",
                [],
            )?;
            tx.execute("UPDATE reels SET status = 'discovered', status_detail = NULL WHERE id = ?1", [reel_id])?;
            Ok(())
        })
    }

    pub fn delete_reel(&self, reel_id: &str) -> Result<()> {
        self.clear_reel_derived(reel_id)?;
        self.with(|c| c.execute("DELETE FROM reels WHERE id = ?1", [reel_id]))?;
        Ok(())
    }
}

impl Database {
    pub fn reset_reel_stages(&self, id: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE reels SET stages = '{}' WHERE id = ?1", [id]))?;
        Ok(())
    }

    /// status: pending | running | done | skipped | failed
    pub fn set_reel_stage(&self, id: &str, stage: &str, status: &str, detail: Option<&str>) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE reels SET stages = json_set(COALESCE(NULLIF(stages, ''), '{}'), '$.' || ?2, json_object('status', ?3, 'detail', ?4)) WHERE id = ?1",
            params![id, stage, status, detail],
        ))?;
        Ok(())
    }

    /// `None` = follow the app setting; `Some("auto")` or a locale such as `ja-JP`.
    pub fn set_transcript_override(&self, id: &str, locale: Option<&str>) -> Result<()> {
        self.with(|c| c.execute("UPDATE reels SET transcript_locale_override = ?2 WHERE id = ?1", params![id, locale]))?;
        Ok(())
    }

    pub fn set_transcript_confidence(&self, id: &str, confidence: Option<f64>) -> Result<()> {
        self.with(|c| c.execute("UPDATE reels SET transcript_confidence = ?2 WHERE id = ?1", params![id, confidence]))?;
        Ok(())
    }
}

impl Database {
    /// Moves one source's facts and photos from a provisional place to the place the user chose,
    /// keeping their provenance (OCR blocks, Reel timestamps).
    pub fn move_source_evidence(&self, from_place: &str, to_place: &str, screenshot_id: Option<&str>, reel_id: Option<&str>) -> Result<()> {
        self.transaction(|tx| {
            for table in ["travel_facts", "place_images"] {
                tx.execute(
                    &format!("UPDATE {table} SET place_id = ?2 WHERE place_id = ?1 AND ((?3 IS NOT NULL AND screenshot_id = ?3) OR (?4 IS NOT NULL AND reel_id = ?4))"),
                    params![from_place, to_place, screenshot_id, reel_id],
                )?;
            }
            Ok(())
        })
    }
}
