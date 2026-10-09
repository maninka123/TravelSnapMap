//! Durable user corrections: decisions that re-processing must respect.
//! - Source decisions: "this name on this screenshot/Reel is not a place" (a removed pin or a dismissed question).
//! - Place aliases: a map identifier that now means another place (merged into it, or a corrected location).

use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use super::{new_id, now, Database, PlaceRecord, PLACE_COLUMNS};
use crate::text::normalize;

impl Database {
    /// Remembers that `name` found on this screenshot or Reel is not a place to pin.
    pub fn reject_source_name(&self, screenshot_id: Option<&str>, reel_id: Option<&str>, name: &str) -> Result<()> {
        let key = normalize(name);
        if key.is_empty() || (screenshot_id.is_none() && reel_id.is_none()) {
            return Ok(());
        }
        self.with(|c| c.execute(
            "INSERT OR IGNORE INTO source_decisions(id, screenshot_id, reel_id, extracted_name, name_key, decision, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 'rejected', ?6)",
            params![new_id(), screenshot_id, reel_id, name.trim(), key, now()],
        ))?;
        Ok(())
    }

    /// Names rejected for this screenshot or Reel (original spelling).
    pub fn rejected_source_names(&self, screenshot_id: Option<&str>, reel_id: Option<&str>) -> Result<Vec<String>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT extracted_name FROM source_decisions WHERE decision = 'rejected' \
                 AND COALESCE(screenshot_id, '') = COALESCE(?1, '') AND COALESCE(reel_id, '') = COALESCE(?2, '')",
            )?;
            let rows = stmt.query_map(params![screenshot_id, reel_id], |r| r.get(0))?;
            rows.collect()
        })
    }

    /// Takes back a rejection (e.g. the user later picks that name as a place after all).
    pub fn clear_source_rejection(&self, screenshot_id: Option<&str>, reel_id: Option<&str>, name: &str) -> Result<()> {
        self.with(|c| c.execute(
            "DELETE FROM source_decisions WHERE name_key = ?3 \
             AND COALESCE(screenshot_id, '') = COALESCE(?1, '') AND COALESCE(reel_id, '') = COALESCE(?2, '')",
            params![screenshot_id, reel_id, normalize(name)],
        ))?;
        Ok(())
    }

    /// `map_identifier` should resolve to `place_id` from now on. No-op for the place's own current identifier.
    pub fn add_place_alias(&self, map_identifier: &str, place_id: &str, reason: &str) -> Result<()> {
        if map_identifier.trim().is_empty() {
            return Ok(());
        }
        self.with(|c| c.execute(
            "INSERT INTO place_aliases(map_identifier, place_id, reason, created_at) VALUES (?1, ?2, ?3, ?4) \
             ON CONFLICT(map_identifier) DO UPDATE SET place_id = excluded.place_id, reason = excluded.reason",
            params![map_identifier, place_id, reason, now()],
        ))?;
        Ok(())
    }

    /// The place a map identifier was merged or moved into, if any.
    pub fn place_by_alias(&self, map_identifier: &str) -> Result<Option<PlaceRecord>> {
        self.with(|c| c.query_row(
            &format!("SELECT {PLACE_COLUMNS} FROM places p JOIN place_aliases a ON a.place_id = p.id WHERE a.map_identifier = ?1"),
            [map_identifier],
            PlaceRecord::from_row,
        ).optional())
    }
}
