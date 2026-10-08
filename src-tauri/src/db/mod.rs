//! SQLite persistence (`DatabaseService`). All SQL lives in this module.

pub mod records;
mod memories_repo;
mod overrides_repo;
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
pub use memories_repo::NewMemory;
pub use reels_repo::FactProvenance;
pub use runs_repo::{CostSummary, RunRecord, RunReport};
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

pub const SCHEMA_VERSION: i64 = 7;

/// (schema version reached, SQL). Append new steps; never edit a shipped one.
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("schema.sql")),
    (2, include_str!("migration_v2.sql")),
    (3, include_str!("migration_v3.sql")),
    (4, include_str!("migration_v4.sql")),
    (5, include_str!("migration_v5.sql")),
    (6, include_str!("migration_v6.sql")),
    (7, include_str!("migration_v7.sql")),
];

/// Library open failures, worded for people rather than developers.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("This library was created by a newer version of TravelSnapMap (library v{found}; this app supports up to v{supported}). Please update the app.")]
    TooNew { found: i64, supported: i64 },
    #[error("TravelSnapMap couldn't upgrade your library (step v{to}, starting from v{from}): {reason}.\n\nNothing was changed: your original library is intact{backup}. Please report this problem.")]
    MigrationFailed { from: i64, to: i64, reason: String, backup: String },
    #[error("TravelSnapMap couldn't make a safety copy of your library before upgrading it ({0}), so it didn't upgrade. Check free disk space and try again.")]
    BackupFailed(String),
}

impl Database {
    /// Opens (and if needed upgrades) the library. Before any schema migration the database is copied to
    /// `backups/pre-migration-v{old}-to-v{new}-{time}.sqlite`; each migration step is one transaction, so a
    /// failure leaves the original library exactly as it was.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(OpenError::TooNew { found: version, supported: SCHEMA_VERSION }.into());
        }
        let backup = if version > 0 && version < SCHEMA_VERSION {
            Some(Self::backup_before_migration(&conn, path, version)?)
        } else {
            None
        };
        if let Err((failed_step, reason)) = Self::migrate(&mut conn, version) {
            return Err(OpenError::MigrationFailed {
                from: version,
                to: failed_step,
                reason: reason.to_string(),
                backup: backup.map(|b| format!(", and a copy was saved at {}", b.display())).unwrap_or_default(),
            }
            .into());
        }
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Self::migrate(&mut conn, 0).map_err(|(_, e)| anyhow::anyhow!(e))?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Forward-only migrations, each in its own transaction (rolled back automatically on error).
    fn migrate(conn: &mut Connection, from: i64) -> std::result::Result<(), (i64, rusqlite::Error)> {
        fn apply(conn: &mut Connection, step: i64, sql: &str) -> rusqlite::Result<()> {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", step)?;
            tx.commit() // dropping `tx` on error rolls the whole step back
        }
        for (step, sql) in MIGRATIONS.iter().filter(|(v, _)| *v > from) {
            apply(conn, *step, sql).map_err(|e| (*step, e))?;
        }
        Ok(())
    }

    /// Consistent snapshot of the open database (VACUUM INTO) — safe while the app is running.
    pub fn snapshot_to(&self, dest: &Path) -> Result<()> {
        let _ = std::fs::remove_file(dest);
        self.with(|c| c.execute("VACUUM INTO ?1", [dest.to_string_lossy()]))?;
        Ok(())
    }

    /// Consistent copy of the database (works while it's open) next to it, in `backups/`.
    fn backup_before_migration(conn: &Connection, path: &Path, from: i64) -> Result<std::path::PathBuf> {
        let dir = path.parent().unwrap_or(Path::new(".")).join("backups");
        std::fs::create_dir_all(&dir)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let dest = dir.join(format!("pre-migration-v{from}-to-v{SCHEMA_VERSION}-{stamp}.sqlite"));
        conn.execute("VACUUM INTO ?1", [dest.to_string_lossy()])
            .map_err(|e| OpenError::BackupFailed(e.to_string()))?;
        log::info!("library backed up before migration to {}", dest.display());
        Ok(dest)
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
                travel_screenshots: count("SELECT COUNT(*) FROM screenshots WHERE (classification = 'travel' OR user_classification = 'travel') AND status != 'ignored'")?,
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

#[cfg(test)]
mod migration_tests {
    use super::*;

    #[test]
    fn migrations_end_at_the_schema_version() {
        assert_eq!(MIGRATIONS.last().unwrap().0, SCHEMA_VERSION);
    }

    #[test]
    fn failed_upgrade_keeps_the_original_library_and_a_backup() {
        let dir = std::env::temp_dir().join(format!("tsm-migrate-{}", uuid::Uuid::new_v4()));
        let path = dir.join("travelsnapmap.sqlite");
        {
            let db = Database::open(&path).unwrap();
            db.set_setting("marker", Some("my data")).unwrap();
            // Pretend this library is one version older: re-running v6 then fails ("duplicate column").
            db.with(|c| c.execute_batch("PRAGMA user_version = 5")).unwrap();
        }
        let err = Database::open(&path).err().expect("upgrade must fail").to_string();
        assert!(err.contains("original library is intact") && err.contains(&format!("pre-migration-v5-to-v{SCHEMA_VERSION}")), "{err}");

        {
            let conn = Connection::open(&path).unwrap();
            let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
            let marker: String = conn.query_row("SELECT value FROM settings WHERE key = 'marker'", [], |r| r.get(0)).unwrap();
            assert_eq!((version, marker.as_str()), (5, "my data"), "the failed step was rolled back");
        } // closed before cleanup (Windows can't delete open files)
        let backups: Vec<_> = std::fs::read_dir(dir.join("backups")).unwrap().collect();
        assert_eq!(backups.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A real v6 library (the last shipped version) with places, trips and reviews upgrades without losing anything.
    #[test]
    fn v6_library_upgrades_with_all_data() {
        let dir = std::env::temp_dir().join(format!("tsm-v6-{}", uuid::Uuid::new_v4()));
        let path = dir.join("travelsnapmap.sqlite");
        std::fs::create_dir_all(&dir).unwrap();
        {
            let mut conn = Connection::open(&path).unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
            for (step, sql) in MIGRATIONS.iter().filter(|(v, _)| *v <= 6) {
                let tx = conn.transaction().unwrap();
                tx.execute_batch(sql).unwrap();
                tx.pragma_update(None, "user_version", step).unwrap();
                tx.commit().unwrap();
            }
            conn.execute_batch(
                "INSERT INTO places(id, canonical_name, latitude, longitude, map_identifier, notes, created_at, updated_at) \
                   VALUES ('p1', 'Fushimi Inari Taisha', 34.9671, 135.7727, 'm-fushimi', 'go early', '2025-01-01', '2025-01-01');
                 INSERT INTO trips(id, name, created_at) VALUES ('t1', 'Japan', '2025-01-01');
                 INSERT INTO trip_places(id, trip_id, place_id, position, day) VALUES ('tp1', 't1', 'p1', 0, 2);
                 INSERT INTO review_items(id, kind, message, is_resolved, resolution, created_at) \
                   VALUES ('r1', 'placeResolution', 'm', 1, 'chose place', '2025-01-01');",
            ).unwrap();
        }
        let db = Database::open(&path).unwrap();
        let (version, notes, day, reviews): (i64, String, i64, i64) = db.with(|c| Ok((
            c.query_row("PRAGMA user_version", [], |r| r.get(0))?,
            c.query_row("SELECT notes FROM places WHERE id = 'p1'", [], |r| r.get(0))?,
            c.query_row("SELECT day FROM trip_places WHERE id = 'tp1'", [], |r| r.get(0))?,
            c.query_row("SELECT COUNT(*) FROM review_items", [], |r| r.get(0))?,
        ))).unwrap();
        assert_eq!((version, notes.as_str(), day, reviews), (SCHEMA_VERSION, "go early", 2, 1));
        let new_tables: i64 = db.with(|c| c.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('source_decisions', 'place_aliases')", [], |r| r.get(0),
        )).unwrap();
        assert_eq!(new_tables, 2);
        drop(db);
        assert_eq!(std::fs::read_dir(dir.join("backups")).unwrap().count(), 1, "a safety copy is made before upgrading");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn newer_library_is_refused_without_changes() {
        let dir = std::env::temp_dir().join(format!("tsm-newer-{}", uuid::Uuid::new_v4()));
        let path = dir.join("travelsnapmap.sqlite");
        std::fs::create_dir_all(&dir).unwrap();
        Connection::open(&path).unwrap().execute_batch("PRAGMA user_version = 99").unwrap();
        let err = Database::open(&path).err().unwrap().to_string();
        assert!(err.contains("newer version"), "{err}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
