//! Scan/validation runs and their reports; per-screenshot measurements.

use std::collections::HashSet;

use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

use super::{new_id, now, Database};
use crate::services::native::AssetInfo;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub id: String,
    pub kind: String,
    pub requested: i64,
    pub sample: String,
    pub screenshot_count: i64,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRow {
    pub screenshot_id: String,
    pub thumbnail_path: Option<String>,
    pub creation_date: Option<String>,
    pub status: String,
    pub status_detail: Option<String>,
    pub source_type: String,
    pub travel_confidence: f64,
    pub escalation_level: i64,
    pub places: Vec<String>,
    pub places_extracted: i64,
    pub places_auto_resolved: i64,
    pub ocr_ms: i64,
    pub ai_ms: i64,
    pub pipeline_ms: i64,
    pub ai_cost: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunReport {
    pub run: RunRecord,
    pub total: i64,
    pub processed: i64,
    pub travel: i64,
    pub not_travel: i64,
    pub failed: i64,
    pub needs_review: i64,
    pub waiting: i64,
    pub pending: i64,
    /// Stopped by the local filter — no AI request made.
    pub skipped_locally: i64,
    pub ai_requests: i64,
    pub places_found: i64,
    pub places_extracted: i64,
    pub places_auto_resolved: i64,
    /// Extracted places that became verified pins automatically.
    pub resolution_rate: f64,
    pub avg_ocr_ms: f64,
    pub avg_ai_ms: f64,
    pub avg_total_ms: f64,
    pub total_cost: f64,
    pub cost_per_travel_screenshot: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub rows: Vec<RunRow>,
}

impl Database {
    /// Every PhotoKit asset ID already in the database (processed or queued).
    pub fn known_photo_ids(&self) -> Result<HashSet<String>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT photos_id FROM screenshots")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect()
        })
    }

    /// Inserts the given assets and returns the screenshot ids of the ones that were new.
    pub fn insert_assets(&self, assets: &[AssetInfo]) -> Result<Vec<String>> {
        self.transaction(|tx| {
            let mut ids = Vec::new();
            let mut insert = tx.prepare(
                "INSERT OR IGNORE INTO screenshots(id, photos_id, creation_date, width, height, discovered_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for a in assets {
                let id = new_id();
                if insert.execute(params![id, a.id, a.creation_date, a.width, a.height, now()])? > 0 {
                    ids.push(id);
                }
            }
            Ok(ids)
        })
    }

    /// Known screenshots whose processing was interrupted (app quit, offline); failed ones excluded.
    pub fn resumable_screenshot_ids(&self) -> Result<Vec<String>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id FROM screenshots WHERE status NOT IN ('complete','notTravel','needsReview','ignored','failed') ORDER BY creation_date DESC",
            )?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect()
        })
    }

    pub fn set_ocr_ms(&self, id: &str, ms: i64) -> Result<()> {
        self.with(|c| c.execute("UPDATE screenshots SET ocr_ms = ?2 WHERE id = ?1", params![id, ms]))?;
        Ok(())
    }

    pub fn set_pipeline_ms(&self, id: &str, ms: i64) -> Result<()> {
        self.with(|c| c.execute("UPDATE screenshots SET pipeline_ms = ?2 WHERE id = ?1", params![id, ms]))?;
        Ok(())
    }

    /// `table` is "screenshots" or "reels".
    pub fn set_place_counts(&self, table: &str, id: &str, extracted: i64, auto_resolved: i64) -> Result<()> {
        let table = if table == "reels" { "reels" } else { "screenshots" };
        self.with(|c| c.execute(
            &format!("UPDATE {table} SET places_extracted = ?2, places_auto_resolved = ?3 WHERE id = ?1"),
            params![id, extracted, auto_resolved],
        ))?;
        Ok(())
    }

    pub fn create_run(&self, kind: &str, requested: i64, sample: &str) -> Result<String> {
        let id = new_id();
        self.with(|c| c.execute(
            "INSERT INTO test_runs(id, kind, requested, sample, started_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, kind, requested, sample, now()],
        ))?;
        Ok(id)
    }

    pub fn set_run_screenshots(&self, id: &str, ids: &[String]) -> Result<()> {
        self.with(|c| c.execute("UPDATE test_runs SET screenshot_ids = ?2 WHERE id = ?1", params![id, serde_json::to_string(ids).unwrap()]))?;
        Ok(())
    }

    pub fn finish_run(&self, id: &str) -> Result<()> {
        self.with(|c| c.execute("UPDATE test_runs SET finished_at = ?2 WHERE id = ?1", params![id, now()]))?;
        Ok(())
    }

    pub fn runs(&self) -> Result<Vec<RunRecord>> {
        self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT id, kind, requested, sample, (SELECT COUNT(*) FROM json_each(screenshot_ids)), started_at, finished_at \
                 FROM test_runs ORDER BY started_at DESC LIMIT 50",
            )?;
            let rows = stmt.query_map([], |r| Ok(RunRecord {
                id: r.get(0)?, kind: r.get(1)?, requested: r.get(2)?, sample: r.get(3)?, screenshot_count: r.get(4)?,
                started_at: r.get(5)?, finished_at: r.get(6)?,
            }))?;
            rows.collect()
        })
    }

    pub fn run_report(&self, run_id: &str) -> Result<Option<RunReport>> {
        let Some(run) = self.runs()?.into_iter().find(|r| r.id == run_id) else { return Ok(None) };
        let ids_json: String = self.with(|c| c.query_row("SELECT screenshot_ids FROM test_runs WHERE id = ?1", [run_id], |r| r.get(0)).optional())?
            .unwrap_or_else(|| "[]".into());

        let rows: Vec<RunRow> = self.with(|c| {
            let mut stmt = c.prepare(
                "SELECT s.id, s.thumbnail_path, s.creation_date, s.status, s.status_detail, s.source_type, s.travel_confidence, \
                        s.escalation_level, s.places_extracted, s.places_auto_resolved, s.ocr_ms, s.ai_latency_ms, s.pipeline_ms, s.ai_cost, \
                        COALESCE((SELECT group_concat(p.canonical_name, '|') FROM place_screenshots ps JOIN places p ON p.id = ps.place_id \
                                  WHERE ps.screenshot_id = s.id), '') \
                 FROM screenshots s WHERE s.id IN (SELECT value FROM json_each(?1)) ORDER BY s.creation_date DESC",
            )?;
            let rows = stmt.query_map([&ids_json], |r| {
                let names: String = r.get(14)?;
                Ok(RunRow {
                    screenshot_id: r.get(0)?, thumbnail_path: r.get(1)?, creation_date: r.get(2)?, status: r.get(3)?,
                    status_detail: r.get(4)?, source_type: r.get(5)?, travel_confidence: r.get(6)?, escalation_level: r.get(7)?,
                    places_extracted: r.get(8)?, places_auto_resolved: r.get(9)?, ocr_ms: r.get(10)?, ai_ms: r.get(11)?,
                    pipeline_ms: r.get(12)?, ai_cost: r.get(13)?,
                    places: names.split('|').filter(|n| !n.is_empty()).map(String::from).collect(),
                })
            })?;
            rows.collect()
        })?;
        let (input_tokens, output_tokens, places_found): (i64, i64, i64) = self.with(|c| c.query_row(
            "SELECT COALESCE(SUM(ai_input_tokens),0), COALESCE(SUM(ai_output_tokens),0), \
                    (SELECT COUNT(DISTINCT place_id) FROM place_screenshots WHERE screenshot_id IN (SELECT value FROM json_each(?1))) \
             FROM screenshots WHERE id IN (SELECT value FROM json_each(?1))",
            [&ids_json],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        ))?;

        let count = |f: &dyn Fn(&RunRow) -> bool| rows.iter().filter(|r| f(r)).count() as i64;
        let avg = |f: &dyn Fn(&RunRow) -> i64| {
            let v: Vec<i64> = rows.iter().map(f).filter(|v| *v > 0).collect();
            if v.is_empty() { 0.0 } else { v.iter().sum::<i64>() as f64 / v.len() as f64 }
        };
        let terminal = ["complete", "notTravel", "needsReview", "ignored", "failed"];
        let travel = count(&|r| r.status == "complete" || r.status == "needsReview");
        let places_extracted: i64 = rows.iter().map(|r| r.places_extracted).sum();
        let places_auto_resolved: i64 = rows.iter().map(|r| r.places_auto_resolved).sum();
        let total_cost: f64 = rows.iter().map(|r| r.ai_cost).sum();

        Ok(Some(RunReport {
            total: rows.len() as i64,
            processed: count(&|r| terminal.contains(&r.status.as_str())),
            travel,
            not_travel: count(&|r| r.status == "notTravel"),
            failed: count(&|r| r.status == "failed"),
            needs_review: count(&|r| r.status == "needsReview"),
            waiting: count(&|r| r.status == "waitingForNetwork"),
            pending: count(&|r| !terminal.contains(&r.status.as_str()) && r.status != "waitingForNetwork"),
            skipped_locally: count(&|r| r.escalation_level == 1),
            ai_requests: count(&|r| r.escalation_level >= 2),
            places_found,
            places_extracted,
            places_auto_resolved,
            resolution_rate: if places_extracted > 0 { places_auto_resolved as f64 / places_extracted as f64 } else { 0.0 },
            avg_ocr_ms: avg(&|r| r.ocr_ms),
            avg_ai_ms: avg(&|r| r.ai_ms),
            avg_total_ms: avg(&|r| r.pipeline_ms),
            total_cost,
            cost_per_travel_screenshot: if travel > 0 { total_cost / travel as f64 } else { 0.0 },
            input_tokens,
            output_tokens,
            rows,
            run,
        }))
    }
}
