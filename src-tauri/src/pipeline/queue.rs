//! Background processing queue: discovery, bounded concurrency, pause/resume/cancel,
//! progress reporting and restart recovery (state lives in SQLite, not memory).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::Semaphore;

use super::Pipeline;
use crate::models::ProcessingStatus;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueSnapshot {
    pub running: bool,
    pub paused: bool,
    pub phase: String,
    pub discovered: i64,
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

    /// Discovers new screenshots (optional) and processes everything pending.
    /// Only unprocessed, interrupted, offline-parked or failed screenshots are picked up.
    pub async fn run(self: Arc<Self>, discover: bool, limit: Option<i64>) {
        if self.state.lock().unwrap().running {
            return;
        }
        self.cancel.store(false, Ordering::SeqCst);
        self.paused.store(false, Ordering::SeqCst);
        self.update(|s| *s = QueueSnapshot { running: true, phase: "Starting…".into(), ..Default::default() });

        let pipeline = self.pipeline.clone();
        let config = pipeline.config();

        if discover {
            self.update(|s| s.phase = "Finding screenshots in Photos…".into());
            match pipeline.photos.list_screenshots(config.scan_from_year, config.scan_to_year).await {
                Ok(assets) => {
                    let new = pipeline.db.upsert_discovered(&assets).unwrap_or(0) as i64;
                    self.update(|s| { s.discovered = assets.len() as i64; s.newly_discovered = new; });
                }
                Err(e) => {
                    self.update(|s| { s.last_error = Some(e.to_string()); s.running = false; s.phase = "Could not read Photos".into(); });
                    return;
                }
            }
        }

        let ids = pipeline.db.screenshot_ids_needing_processing(limit).unwrap_or_default();
        self.update(|s| { s.total = ids.len() as i64; s.remaining = ids.len() as i64; s.phase = "Processing…".into(); });

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

        let cancelled = self.cancel.load(Ordering::SeqCst);
        self.update(|s| {
            s.running = false;
            s.paused = false;
            s.phase = if cancelled { "Stopped".into() } else { "Done".into() };
        });
    }
}
