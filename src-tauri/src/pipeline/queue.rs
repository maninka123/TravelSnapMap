//! Background processing queue: discovery, bounded concurrency, pause/resume/cancel,
//! progress reporting and restart recovery (state lives in SQLite, not memory).

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use super::Pipeline;
use crate::models::ProcessingStatus;
use crate::services::native::AssetInfo;

/// What a run should process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RunMode {
    /// Only screenshots whose PhotoKit ID isn't in the database yet (plus interrupted ones).
    ScanNew,
    /// Test mode: process `count` screenshots not processed before, newest first or a random sample.
    Validation { count: usize, random: bool },
    /// Everything known that still needs work, including failed screenshots.
    Retry,
    /// Only new image files in one folder (screenshots from any device).
    ScanFolder { path: String },
    /// Only work parked for the network or the daily AI limit (never failed items — no repeated AI spend).
    Waiting,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueSnapshot {
    pub running: bool,
    pub paused: bool,
    pub mode: String,
    pub run_id: Option<String>,
    pub phase: String,
    /// Screenshots in the Photos library.
    pub in_photos: i64,
    /// Of those, already known to the app before this run.
    pub already_known: i64,
    pub newly_discovered: i64,
    pub total: i64,
    pub processed: i64,
    pub travel: i64,
    pub not_travel: i64,
    pub needs_review: i64,
    pub failed: i64,
    pub waiting: i64,
    pub remaining: i64,
    pub last_error: Option<String>,
}

pub type ProgressCallback = Arc<dyn Fn(&QueueSnapshot) + Send + Sync>;

pub struct ProcessingQueue {
    pipeline: Arc<Pipeline>,
    state: Arc<Mutex<QueueSnapshot>>,
    cancel: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    on_progress: ProgressCallback,
}

/// Picks which new assets a run processes. Deterministic except for the random sample.
pub fn select_assets(new_assets: Vec<AssetInfo>, mode: &RunMode) -> Vec<AssetInfo> {
    match mode {
        RunMode::Validation { count, random } => {
            let mut assets = new_assets;
            if *random {
                // Fisher–Yates with a time seed: a spread across years gives a more honest sample.
                let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(7) | 1;
                for i in (1..assets.len()).rev() {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    assets.swap(i, (seed % (i as u64 + 1)) as usize);
                }
            }
            assets.truncate(*count);
            assets
        }
        RunMode::ScanNew | RunMode::ScanFolder { .. } => new_assets,
        RunMode::Retry | RunMode::Waiting => vec![],
    }
}

impl ProcessingQueue {
    pub fn new(pipeline: Arc<Pipeline>, on_progress: ProgressCallback) -> Self {
        Self {
            pipeline,
            state: Arc::new(Mutex::new(QueueSnapshot::default())),
            cancel: Arc::new(AtomicBool::new(false)),
            paused: Arc::new(AtomicBool::new(false)),
            on_progress,
        }
    }

    pub fn snapshot(&self) -> QueueSnapshot {
        self.state.lock().unwrap().clone()
    }

    fn update(&self, f: impl FnOnce(&mut QueueSnapshot)) {
        let snap = {
            let mut s = self.state.lock().unwrap();
            f(&mut s);
            s.clone()
        };
        (self.on_progress)(&snap);
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        self.update(|s| { s.paused = true; s.phase = "Paused".into(); });
    }

    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.update(|s| { s.paused = false; s.phase = "Processing…".into(); });
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
        self.update(|s| s.phase = "Stopping after current screenshots…".into());
    }

    /// Runs one scan. Returns the run id (for Scan New and Validation) when finished.
    pub async fn run(self: Arc<Self>, mode: RunMode) -> Option<String> {
        {
            let mut s = self.state.lock().unwrap();
            if s.running {
                return None;
            }
            s.running = true;
        }
        self.cancel.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
        let mode_name = match &mode {
            RunMode::ScanNew => "scanNew".to_string(),
            RunMode::Validation { count, .. } => format!("validation:{count}"),
            RunMode::Retry => "retry".to_string(),
            RunMode::ScanFolder { .. } => "scanFolder".to_string(),
            RunMode::Waiting => "waiting".to_string(),
        };
        self.update(|s| *s = QueueSnapshot { running: true, mode: mode_name, phase: "Starting…".into(), ..Default::default() });

        let pipeline = self.pipeline.clone();
        let db = pipeline.db.clone();
        let config = pipeline.config();

        let (ids, run_id) = if mode == RunMode::Retry {
            (db.screenshot_ids_needing_processing(None).unwrap_or_default(), None)
        } else if mode == RunMode::Waiting {
            (db.resumable_screenshot_ids().unwrap_or_default(), None)
        } else {
            self.update(|s| s.phase = "Checking for new screenshots…".into());
            let assets = match discover(&pipeline, &mode, &config).await {
                Ok((a, warning)) => {
                    if let Some(w) = warning {
                        self.update(|s| s.last_error = Some(w));
                    }
                    a
                }
                Err(e) => {
                    self.update(|s| { s.last_error = Some(e.to_string()); s.running = false; s.phase = "Could not read screenshots".into(); });
                    return None;
                }
            };
            // Compare PhotoKit asset IDs with the database; only unknown ones are new.
            let known: HashSet<String> = db.known_photo_ids().unwrap_or_default();
            let in_photos = assets.len() as i64;
            let already_known = assets.iter().filter(|a| known.contains(&a.id)).count() as i64;
            let new_assets: Vec<AssetInfo> = assets.into_iter().filter(|a| !known.contains(&a.id)).collect();
            let new_count = new_assets.len() as i64;
            let selected = select_assets(new_assets, &mode);
            let mut ids = db.insert_assets(&selected).unwrap_or_default();
            if mode == RunMode::ScanNew {
                // Also finish anything a previous run left half-done (not failed ones — that's Retry).
                let fresh: HashSet<String> = ids.iter().cloned().collect();
                ids.extend(db.resumable_screenshot_ids().unwrap_or_default().into_iter().filter(|i| !fresh.contains(i)));
            }
            let (kind, requested, sample) = match &mode {
                RunMode::Validation { count, random } => ("validation", *count as i64, if *random { "random" } else { "newest" }),
                RunMode::ScanFolder { .. } => ("scanFolder", ids.len() as i64, "new"),
                _ => ("scanNew", ids.len() as i64, "new"),
            };
            let run_id = db.create_run(kind, requested, sample).ok();
            if let Some(r) = &run_id {
                let _ = db.set_run_screenshots(r, &ids);
            }
            self.update(|s| { s.in_photos = in_photos; s.already_known = already_known; s.newly_discovered = new_count; s.run_id = run_id.clone(); });
            (ids, run_id)
        };

        self.update(|s| {
            s.total = ids.len() as i64;
            s.remaining = ids.len() as i64;
            s.phase = if ids.is_empty() { "No new screenshots".into() } else { "Processing…".into() };
        });

        let semaphore = Arc::new(Semaphore::new(config.max_concurrent_screenshots.max(1)));
        let mut tasks = tokio::task::JoinSet::new();
        for id in ids {
            // Pause: hold new work (in-flight screenshots finish normally).
            while self.paused.load(Ordering::SeqCst) && !self.cancel.load(Ordering::SeqCst) {
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
            if self.cancel.load(Ordering::SeqCst) {
                break;
            }
            let permit = semaphore.clone().acquire_owned().await.expect("semaphore");
            let (pipeline, me) = (pipeline.clone(), self.clone());
            tasks.spawn(async move {
                let status = pipeline.process(&id).await;
                drop(permit);
                me.update(|s| {
                    s.processed += 1;
                    s.remaining = (s.total - s.processed).max(0);
                    match status {
                        ProcessingStatus::Complete => s.travel += 1,
                        ProcessingStatus::NeedsReview => { s.travel += 1; s.needs_review += 1; }
                        ProcessingStatus::NotTravel => s.not_travel += 1,
                        ProcessingStatus::Failed => s.failed += 1,
                        ProcessingStatus::WaitingForNetwork => s.waiting += 1,
                        _ => {}
                    }
                });
            });
        }
        while tasks.join_next().await.is_some() {}

        if let Some(r) = &run_id {
            let _ = db.finish_run(r);
        }
        let cancelled = self.cancel.load(Ordering::SeqCst);
        self.update(|s| {
            s.running = false;
            s.paused = false;
            if s.total > 0 {
                s.phase = if cancelled { "Stopped".into() } else { "Done".into() };
            }
        });
        run_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assets(n: usize) -> Vec<AssetInfo> {
        (0..n).map(|i| AssetInfo { id: format!("a{i}"), creation_date: None, width: 0, height: 0 }).collect()
    }

    #[test]
    fn validation_selects_the_requested_count() {
        assert_eq!(select_assets(assets(300), &RunMode::Validation { count: 25, random: false }).len(), 25);
        let newest = select_assets(assets(300), &RunMode::Validation { count: 10, random: false });
        assert_eq!(newest[0].id, "a0");
        let random = select_assets(assets(300), &RunMode::Validation { count: 100, random: true });
        let unique: HashSet<_> = random.iter().map(|a| a.id.clone()).collect();
        assert_eq!(unique.len(), 100);
        assert_eq!(select_assets(assets(5), &RunMode::Validation { count: 50, random: true }).len(), 5);
        assert_eq!(select_assets(assets(7), &RunMode::ScanNew).len(), 7);
    }
}

/// Lists candidate assets for a run: Apple Photos and/or screenshot folders.
/// Returns the assets and an optional warning (e.g. Photos not authorised but folders were scanned).
async fn discover(pipeline: &Pipeline, mode: &RunMode, config: &crate::config::AppConfig) -> anyhow::Result<(Vec<AssetInfo>, Option<String>)> {
    let scan = |path: String| async move {
        tokio::task::spawn_blocking(move || crate::services::folder::scan_folder(std::path::Path::new(&path)))
            .await
            .map_err(|e| anyhow::anyhow!(e))?
            .map_err(anyhow::Error::from)
    };
    match mode {
        RunMode::ScanFolder { path } => Ok((scan(path.clone()).await?, None)),
        RunMode::Validation { .. } => Ok((pipeline.photos.list_screenshots(config.scan_from_year, config.scan_to_year).await?, None)),
        RunMode::ScanNew => {
            let mut assets = Vec::new();
            let mut warning = None;
            match pipeline.photos.list_screenshots(config.scan_from_year, config.scan_to_year).await {
                Ok(a) => assets.extend(a),
                Err(e) if !config.screenshot_folders.is_empty() => warning = Some(format!("Photos skipped: {e}")),
                Err(e) => return Err(e),
            }
            for folder in &config.screenshot_folders {
                match scan(folder.clone()).await {
                    Ok(a) => assets.extend(a),
                    Err(e) => warning = Some(format!("Folder {folder} skipped: {e}")),
                }
            }
            Ok((assets, warning))
        }
        RunMode::Retry | RunMode::Waiting => Ok((vec![], None)),
    }
}
