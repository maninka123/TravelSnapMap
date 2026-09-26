//! After-trip memories: your own photos from Photos attached to a visited place, plus the
//! cross-place reads used by Smart Search and the freshness/conflict warnings.

use anyhow::Result;
use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

use super::records::{FactRecord, FACT_COLUMNS};
use super::{new_id, now, Database};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecord {
    pub id: String,
    pub place_id: String,
    pub photos_id: String,
    pub taken_at: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub image_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub created_at: String,
}

const MEMORY_COLUMNS: &str = "id, place_id, photos_id, taken_at, latitude, longitude, image_path, thumbnail_path, created_at";

impl MemoryRecord {
    fn from_row(r: &Row) -> rusqlite::Result<Self> {
        Ok(Self {
            id: r.get(0)?,
            place_id: r.get(1)?,
            photos_id: r.get(2)?,
            taken_at: r.get(3)?,
            latitude: r.get(4)?,
            longitude: r.get(5)?,
            image_path: r.get(6)?,
            thumbnail_path: r.get(7)?,
            created_at: r.get(8)?,
        })
    }
}

/// Where a place's evidence came from, for "saved from Instagram" style searches.
#[derive(Debug, Clone, Default)]
pub struct PlaceProvenance {
    pub source_types: Vec<String>,
    pub creators: Vec<String>,
    pub has_reel: bool,
    pub has_screenshot: bool,
}

pub struct NewMemory<'a> {
    pub place_id: &'a str,
    pub photos_id: &'a str,
    pub taken_at: Option<&'a str>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub image_path: &'a str,
    pub thumbnail_path: &'a str,
}

impl Database {
    /// Returns the new id, or None when that photo is already attached to the place.
    pub fn insert_memory(&self, m: &NewMemory) -> Result<Option<String>> {
        let id = new_id();
        let n = self.with(|c| c.execute(
            "INSERT OR IGNORE INTO place_memories(id, place_id, photos_id, taken_at, latitude, longitude, image_path, thumbnail_path, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, m.place_id, m.photos_id, m.taken_at, m.latitude, m.longitude, m.image_path, m.thumbnail_path, now()],
        ))?;
        Ok((n > 0).then_some(id))
    }

    pub fn memory_exists(&self, place_id: &str, photos_id: &str) -> Result<bool> {
        Ok(self.with(|c| c.query_row(
            "SELECT 1 FROM place_memories WHERE place_id = ?1 AND photos_id = ?2", params![place_id, photos_id], |_| Ok(()),
        ).optional())?.is_some())
    }

    pub fn memories_for_place(&self, place_id: &str) -> Result<Vec<MemoryRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!("SELECT {MEMORY_COLUMNS} FROM place_memories WHERE place_id = ?1 ORDER BY taken_at, created_at"))?;
            let rows = stmt.query_map([place_id], MemoryRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn memory(&self, id: &str) -> Result<Option<MemoryRecord>> {
        self.with(|c| c.query_row(&format!("SELECT {MEMORY_COLUMNS} FROM place_memories WHERE id = ?1"), [id], MemoryRecord::from_row).optional())
    }

    pub fn delete_memory(&self, id: &str) -> Result<()> {
        self.transaction(|tx| {
            tx.execute("UPDATE places SET cover_memory_id = NULL WHERE cover_memory_id = ?1", [id])?;
            tx.execute("DELETE FROM place_memories WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    /// Marks a place visited; the visit date is only filled in when you haven't set one.
    pub fn mark_visited(&self, place_id: &str, visited_at: Option<&str>) -> Result<()> {
        self.with(|c| c.execute(
            "UPDATE places SET personal_status = CASE WHEN personal_status = 'favourite' THEN personal_status ELSE 'visited' END, \
             visited_at = COALESCE(visited_at, ?2), updated_at = ?3 WHERE id = ?1",
            params![place_id, visited_at, now()],
        ))?;
        Ok(())
    }

    /// Every fact in the library (one query), for search and warnings.
    pub fn all_facts(&self) -> Result<Vec<FactRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(&format!(
                "SELECT {FACT_COLUMNS} FROM travel_facts f LEFT JOIN screenshots s ON s.id = f.screenshot_id LEFT JOIN reels rl ON rl.id = f.reel_id \
                 ORDER BY f.place_id, f.type, COALESCE(f.valid_from, f.created_at) DESC"
            ))?;
            let rows = stmt.query_map([], FactRecord::from_row)?;
            rows.collect()
        })
    }

    pub fn place_provenance(&self) -> Result<std::collections::HashMap<String, PlaceProvenance>> {
        let mut map: std::collections::HashMap<String, PlaceProvenance> = std::collections::HashMap::new();
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT ps.place_id, s.source_type, s.creator FROM place_screenshots ps JOIN screenshots s ON s.id = ps.screenshot_id",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?)))?;
            for row in rows {
                let (id, source, creator) = row?;
                let e = map.entry(id).or_default();
                e.has_screenshot = true;
                e.source_types.extend(source);
                e.creators.extend(creator);
            }
            let mut stmt = c.prepare("SELECT pr.place_id, rl.creator FROM place_reels pr JOIN reels rl ON rl.id = pr.reel_id")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)))?;
            for row in rows {
                let (id, creator) = row?;
                let e = map.entry(id).or_default();
                e.has_reel = true;
                e.source_types.push("instagram".into());
                e.creators.extend(creator);
            }
            Ok(())
        })?;
        Ok(map)
    }
}
