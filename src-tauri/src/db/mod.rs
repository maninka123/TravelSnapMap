//! SQLite persistence (`DatabaseService`). All SQL lives in this module.

pub mod records;
mod reels_repo;
mod repo;
mod runs_repo;

use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use crate::config::AppConfig;
use crate::services::ai::types::AiUsage;
pub use records::*;
pub use reels_repo::FactProvenance;
pub use runs_repo::{RunRecord, RunReport};
pub use repo::{PlaceFilter, ScreenshotFilter};

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub struct Database {
    conn: Mutex<Connection>,
}

const SCHEMA_VERSION: i64 = 3;

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        // Forward-only migrations; each step runs once.
        if version < 1 {
            conn.execute_batch(include_str!("schema.sql"))?;
            conn.execute_batch("PRAGMA user_version = 1")?;
        }
        if version < 2 {
            conn.execute_batch(&format!("BEGIN; {} PRAGMA user_version = 2; COMMIT;", include_str!("migration_v2.sql")))?;
        }
        if version < 3 {
            conn.execute_batch(&format!("BEGIN; {} PRAGMA user_version = 3; COMMIT;", include_str!("migration_v3.sql")))?;
        }
        debug_assert!(SCHEMA_VERSION == 3);
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Runs a closure with the connection. Keep closures short: never hold across `.await`.
    pub fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        Ok(f(&conn)?)
    }

    /// Runs a closure in a transaction (all-or-nothing, so a failure never corrupts records).
    pub fn transaction<T>(&self, f: impl FnOnce(&rusqlite::Transaction) -> rusqlite::Result<T>) -> Result<T> {
        let mut conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let tx = conn.transaction()?;
        let value = f(&tx)?;
        tx.commit()?;
        Ok(value)
    }

    // MARK: Settings

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        self.with(|c| c.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional())
    }

    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<()> {
        self.with(|c| match value {
            Some(v) => c.execute("INSERT INTO settings(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2", params![key, v]),
            None => c.execute("DELETE FROM settings WHERE key = ?1", [key]),
        })?;
        Ok(())
    }

    pub fn config(&self) -> AppConfig {
        self.setting("config").ok().flatten()
            .and_then(|s| serde_json::from_str::<AppConfig>(&s).ok())
            .map(AppConfig::upgraded)
            .unwrap_or_default()
    }

    pub fn save_config(&self, config: &AppConfig) -> Result<()> {
        self.set_setting("config", Some(&serde_json::to_string(config)?))
    }

    // MARK: Metrics & AI usage

    pub fn add_metric(&self, key: &str, delta: f64) {
        let _ = self.with(|c| {
            c.execute("INSERT INTO metrics(key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = value + ?2", params![key, delta])
        });
    }

    pub fn metrics(&self) -> Result<Vec<(String, f64)>> {
        self.with(|c| {
            let mut stmt = c.prepare("SELECT key, value FROM metrics ORDER BY key")?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect()
        })
    }

    pub fn log_ai_usage(&self, screenshot_id: Option<&str>, kind: &str, usage: &AiUsage, success: bool, prompt_version: i64) {
        let _ = self.with(|c| {
            c.execute(
                "INSERT INTO ai_usage_log(screenshot_id, kind, model, thinking, image_used, input_tokens, cache_hit_tokens, \
                 output_tokens, latency_ms, retries, cost, success, prompt_version, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![screenshot_id, kind, usage.model, usage.thinking, usage.image_used, usage.input_tokens as i64,
                        usage.cache_hit_tokens as i64, usage.output_tokens as i64, usage.latency_ms as i64,
                        usage.retries as i64, usage.estimated_cost, success, prompt_version, now()],
            )
        });
    }

    /// AI requests made today (local time), for the daily limit.
    pub fn ai_requests_today(&self) -> Result<i64> {
        let start = chrono::Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap()
            .and_local_timezone(chrono::Local).unwrap().with_timezone(&chrono::Utc).to_rfc3339();
        self.with(|c| c.query_row("SELECT COUNT(*) FROM ai_usage_log WHERE created_at >= ?1", [start], |r| r.get(0)))
    }

    pub fn ai_cache_get(&self, ocr_hash: &str, model: &str, prompt_version: i64, kind: &str) -> Result<Option<String>> {
        self.with(|c| {
            c.query_row(
                "SELECT result_json FROM ai_cache WHERE ocr_hash = ?1 AND model = ?2 AND prompt_version = ?3 AND kind = ?4",
                params![ocr_hash, model, prompt_version, kind],
                |r| r.get(0),
            )
            .optional()
        })
    }

    pub fn ai_cache_put(&self, ocr_hash: &str, model: &str, prompt_version: i64, kind: &str, screenshot_id: &str, json: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT OR REPLACE INTO ai_cache(ocr_hash, model, prompt_version, kind, screenshot_id, result_json, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![ocr_hash, model, prompt_version, kind, screenshot_id, json, now()],
            )
        })?;
        Ok(())
    }

    pub fn diagnostics(&self) -> Result<Diagnostics> {
        self.with(|c| {
            let count = |sql: &str| -> rusqlite::Result<i64> { c.query_row(sql, [], |r| r.get(0)) };
            let sum = |sql: &str| -> rusqlite::Result<f64> { c.query_row(sql, [], |r| r.get::<_, Option<f64>>(0)).map(|v| v.unwrap_or(0.0)) };
            Ok(Diagnostics {
                total_screenshots: count("SELECT COUNT(*) FROM screenshots")?,
                travel_screenshots: count("SELECT COUNT(*) FROM screenshots WHERE classification = 'travel' OR user_classification = 'travel'")?,
                not_travel: count("SELECT COUNT(*) FROM screenshots WHERE status = 'notTravel'")?,
                skipped_locally: count("SELECT COUNT(*) FROM screenshots WHERE escalation_level = 1")?,
                needs_review: count("SELECT COUNT(*) FROM screenshots WHERE status = 'needsReview'")?,
                failed: count("SELECT COUNT(*) FROM screenshots WHERE status = 'failed'")?,
                remaining: count("SELECT COUNT(*) FROM screenshots WHERE status NOT IN ('complete','notTravel','needsReview','ignored')")?,
                places: count("SELECT COUNT(*) FROM places")?,
                ai_requests: count("SELECT COUNT(*) FROM ai_usage_log")?,
                ai_text_requests: count("SELECT COUNT(*) FROM ai_usage_log WHERE image_used = 0")?,
                ai_vision_requests: count("SELECT COUNT(*) FROM ai_usage_log WHERE image_used = 1")?,
                ai_thinking_requests: count("SELECT COUNT(*) FROM ai_usage_log WHERE thinking = 1")?,
                ai_failures: count("SELECT COUNT(*) FROM ai_usage_log WHERE success = 0")?,
                ai_input_tokens: count("SELECT COALESCE(SUM(input_tokens),0) FROM ai_usage_log")?,
                ai_output_tokens: count("SELECT COALESCE(SUM(output_tokens),0) FROM ai_usage_log")?,
                ai_cache_hit_tokens: count("SELECT COALESCE(SUM(cache_hit_tokens),0) FROM ai_usage_log")?,
                ai_estimated_cost: sum("SELECT SUM(cost) FROM ai_usage_log")?,
                ai_average_latency_ms: sum("SELECT AVG(latency_ms) FROM ai_usage_log")?,
                open_reviews: count("SELECT COUNT(*) FROM review_items WHERE is_resolved = 0")?,
                metrics: {
                    let mut stmt = c.prepare("SELECT key, value FROM metrics ORDER BY key")?;
                    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                    rows.collect::<rusqlite::Result<Vec<(String, f64)>>>()?
                },
            })
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub total_screenshots: i64,
    pub travel_screenshots: i64,
    pub not_travel: i64,
    pub skipped_locally: i64,
    pub needs_review: i64,
    pub failed: i64,
    pub remaining: i64,
    pub places: i64,
    pub ai_requests: i64,
    pub ai_text_requests: i64,
    pub ai_vision_requests: i64,
    pub ai_thinking_requests: i64,
    pub ai_failures: i64,
    pub ai_input_tokens: i64,
    pub ai_output_tokens: i64,
    pub ai_cache_hit_tokens: i64,
    pub ai_estimated_cost: f64,
    pub ai_average_latency_ms: f64,
    pub open_reviews: i64,
    pub metrics: Vec<(String, f64)>,
}
