//! After-trip memories: your own photos from Photos attached to a visited place, plus the
//! library-wide fact read used by the freshness/conflict warnings.

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
}

/// SQL: the place (`places`) has no screenshot/Reel behind it and nothing you added — an automatic pin that
/// lost its support. Places you added (+ Add Place) or put something into (notes, visit, your photos or tips,
/// memories, a trip, a status) always stay.
const UNSUPPORTED_AUTOMATIC: &str = "origin != 'manual' \
    AND NOT EXISTS (SELECT 1 FROM place_screenshots ps WHERE ps.place_id = places.id) \
    AND NOT EXISTS (SELECT 1 FROM place_reels pr WHERE pr.place_id = places.id) \
    AND notes = '' AND visit_notes = '' AND visited_at IS NULL AND personal_status = 'wantToVisit' \
    AND NOT EXISTS (SELECT 1 FROM place_memories m WHERE m.place_id = places.id) \
    AND NOT EXISTS (SELECT 1 FROM trip_places t WHERE t.place_id = places.id) \
    AND NOT EXISTS (SELECT 1 FROM place_images i WHERE i.place_id = places.id AND i.origin = 'user') \
    AND NOT EXISTS (SELECT 1 FROM travel_facts f WHERE f.place_id = places.id AND f.origin = 'user')";

impl Database {
    /// An automatic pin needs a screenshot or Reel behind it: removes the ones with none left and nothing of
    /// yours (see UNSUPPORTED_AUTOMATIC), and open questions about them. Places created in the last couple of
    /// minutes are left alone, so a scan that's attaching one right now is never interrupted.
    pub fn remove_unsupported_places(&self) -> Result<usize> {
        let settle = (chrono::Utc::now() - chrono::Duration::minutes(2)).to_rfc3339();
        self.transaction(|tx| {
            let n = tx.execute(&format!("DELETE FROM places WHERE created_at < ?1 AND {UNSUPPORTED_AUTOMATIC}"), [&settle])?;
            tx.execute(
                "DELETE FROM review_items WHERE is_resolved = 0 AND ((place_a_id IS NOT NULL AND place_a_id NOT IN (SELECT id FROM places)) \
                 OR (place_b_id IS NOT NULL AND place_b_id NOT IN (SELECT id FROM places)))",
                [],
            )?;
            Ok(n)
        })
    }

    /// Removes this one place if it's an automatic pin with no support and nothing of yours.
    pub fn remove_if_unsupported(&self, place_id: &str) -> Result<()> {
        self.with(|c| c.execute(&format!("DELETE FROM places WHERE id = ?1 AND {UNSUPPORTED_AUTOMATIC}"), [place_id]))?;
        Ok(())
    }

    /// Ignoring a screenshot or Reel: it stays in "Ignored" but no longer affects anything — its place links,
    /// tips (yours too), photos and review items go. Places that only existed because of it are removed,
    /// unless you've put something into them (notes, a visit, your photos, a trip, a status).
    pub fn detach_source(&self, screenshot_id: Option<&str>, reel_id: Option<&str>) -> Result<usize> {
        self.transaction(|tx| {
            let (link_table, column, id) = match (screenshot_id, reel_id) {
                (Some(s), _) => ("place_screenshots", "screenshot_id", s),
                (None, Some(r)) => ("place_reels", "reel_id", r),
                _ => return Ok(0),
            };
            let places: Vec<String> = {
                let mut stmt = tx.prepare(&format!("SELECT place_id FROM {link_table} WHERE {column} = ?1"))?;
                let rows = stmt.query_map([id], |r| r.get(0))?;
                rows.collect::<rusqlite::Result<_>>()?
            };
            tx.execute(&format!("DELETE FROM {link_table} WHERE {column} = ?1"), [id])?;
            tx.execute(&format!("DELETE FROM travel_facts WHERE {column} = ?1"), [id])?;
            tx.execute(&format!("DELETE FROM place_images WHERE {column} = ?1"), [id])?;
            tx.execute(&format!("DELETE FROM review_items WHERE {column} = ?1"), [id])?;
            let mut removed = 0;
            for place in places {
                removed += tx.execute(&format!("DELETE FROM places WHERE id = ?1 AND {UNSUPPORTED_AUTOMATIC}"), [&place])?;
            }
            // Duplicate-place questions that involved a removed place are moot.
            tx.execute(
                "DELETE FROM review_items WHERE (place_a_id IS NOT NULL AND place_a_id NOT IN (SELECT id FROM places)) \
                 OR (place_b_id IS NOT NULL AND place_b_id NOT IN (SELECT id FROM places))",
                [],
            )?;
            Ok(removed)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DataOrigin, PlaceCandidate, PlaceCategory, Verification};

    fn candidate(name: &str, lat: f64) -> PlaceCandidate {
        serde_json::from_value(serde_json::json!({"name": name, "latitude": lat, "longitude": 80.0})).unwrap()
    }

    #[test]
    fn ignoring_a_source_removes_its_effect_but_keeps_places_you_care_about() {
        let dir = std::env::temp_dir().join(format!("tsm-ignore-{}", uuid::Uuid::new_v4()));
        let db = Database::open(&dir.join("t.sqlite")).unwrap();
        db.with(|c| c.execute_batch(
            "INSERT INTO screenshots(id, photos_id, discovered_at) VALUES ('s1','p1','2026-01-01'), ('s2','p2','2026-01-01');",
        )).unwrap();
        let only = db.insert_place(&candidate("Only from s1", 7.0), PlaceCategory::Beach, Verification::UserVerified, DataOrigin::User, &[]).unwrap();
        let shared = db.insert_place(&candidate("Also in s2", 7.5), PlaceCategory::Beach, Verification::Verified, DataOrigin::MapKit, &[]).unwrap();
        let noted = db.insert_place(&candidate("Has my notes", 8.0), PlaceCategory::Beach, Verification::Verified, DataOrigin::MapKit, &[]).unwrap();
        for p in [&only, &shared, &noted] {
            db.link(&p.id, "s1", &p.canonical_name, 1.0, DataOrigin::User).unwrap();
        }
        db.link(&shared.id, "s2", "x", 0.9, DataOrigin::Ai).unwrap();
        db.update_place_field(&noted.id, "notes", Some("go early")).unwrap();

        let removed = db.detach_source(Some("s1"), None).unwrap();
        assert_eq!(removed, 1);
        assert!(db.place(&only.id).unwrap().is_none(), "a place that only came from the ignored screenshot is gone");
        assert!(db.place(&shared.id).unwrap().is_some(), "still backed by another screenshot");
        assert!(db.place(&noted.id).unwrap().is_some(), "you wrote notes on it");
        assert!(db.places_for_screenshot("s1").unwrap().is_empty());

        // A place you added by hand stays with no screenshots; an automatic one without support goes.
        let manual = db.insert_place(&candidate("Added by me", 9.0), PlaceCategory::Beach, Verification::UserVerified, DataOrigin::Manual, &[]).unwrap();
        let stray = db.insert_place(&candidate("Leftover", 9.5), PlaceCategory::Beach, Verification::Verified, DataOrigin::Ai, &[]).unwrap();
        db.with(|c| c.execute("UPDATE places SET created_at = '2020-01-01T00:00:00+00:00'", [])).unwrap();
        db.remove_unsupported_places().unwrap();
        assert!(db.place(&manual.id).unwrap().is_some(), "added by hand");
        assert!(db.place(&stray.id).unwrap().is_none(), "automatic, nothing behind it");
        assert!(db.place(&noted.id).unwrap().is_some(), "still has your notes");
        drop(db); // closed before cleanup (Windows can't delete open files)
        std::fs::remove_dir_all(dir).unwrap();
    }
}
