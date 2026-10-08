//! TravelSnapMap backend: Tauri app setup and dependency wiring.

mod backup;
mod commands;
mod config;
mod countries;
mod db;
mod insights;
mod models;
mod pipeline;
mod secrets;
mod services;
mod text;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::db::Database;
use crate::pipeline::queue::{ProcessingQueue, RunMode};
use crate::pipeline::Pipeline;
use crate::services::ai::deepseek::DeepSeekTravelAIService;
use crate::services::native::{NativeBridge, PhotoLibraryService};
use crate::services::places::PlaceService;
use crate::services::reels::InstagramFetcher;

pub struct AppState {
    pub db: Arc<Database>,
    pub pipeline: Arc<Pipeline>,
    pub queue: Arc<ProcessingQueue>,
}

/// Loads `.env.local` / `.env` from the project root in development (never bundled).
fn load_env() {
    for name in ["../.env.local", ".env.local", "../.env", ".env"] {
        let _ = dotenvy::from_filename(name);
    }
    // The double-clicked .app starts in "/", so also read the project's own .env.local by absolute path.
    // It is read at runtime (never compiled into the app) and simply doesn't exist on other machines.
    let project_env = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.env.local");
    let _ = dotenvy::from_path(project_env);
}

/// A native alert for errors that happen before the window exists (e.g. a failed library upgrade).
fn fatal_alert(title: &str, message: &str) {
    let esc = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let script = format!("display alert \"{}\" message \"{}\" as critical", esc(title), esc(message));
    let _ = std::process::Command::new("osascript").arg("-e").arg(script).status();
}

/// Logs to ~/Library/Logs/TravelSnapMap/travelsnapmap.log (rotated at 5 MB) and to stderr in development.
/// Screenshot contents are never logged.
fn init_logging() {
    let mut builder = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if !cfg!(debug_assertions) {
        if let Ok(home) = std::env::var("HOME") {
            let dir = std::path::PathBuf::from(home).join("Library/Logs/TravelSnapMap");
            let path = dir.join("travelsnapmap.log");
            let _ = std::fs::create_dir_all(&dir);
            if std::fs::metadata(&path).map(|m| m.len() > 5_000_000).unwrap_or(false) {
                let _ = std::fs::rename(&path, dir.join("travelsnapmap.1.log"));
            }
            if let Ok(file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
                builder.target(env_logger::Target::Pipe(Box::new(file)));
            }
        }
    }
    let _ = builder.try_init();
}

pub fn run() {
    load_env();
    init_logging();
    log::info!("TravelSnapMap {} starting", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            // A backup chosen in Settings → Restore replaces the library now, before it is opened.
            match backup::apply_pending_restore(&data_dir) {
                Ok(Some(kept)) => log::info!("backup restored; previous library kept in {}", kept.display()),
                Err(e) => log::error!("could not restore the backup: {e:#}"),
                _ => {}
            }
            let db = match Database::open(&data_dir.join("travelsnapmap.sqlite")) {
                Ok(db) => Arc::new(db),
                Err(e) => {
                    log::error!("could not open library: {e:#}");
                    fatal_alert("TravelSnapMap can't open your library", &e.to_string());
                    std::process::exit(1);
                }
            };
            // None = never saved (fresh install); Some(0) = saved before settings were versioned.
            let saved_version = db.setting("config")?.map(|s| serde_json::from_str::<serde_json::Value>(&s).ok()
                .and_then(|v| v["configVersion"].as_u64()).unwrap_or(0));
            let config = db.config();
            if saved_version.is_some_and(|v| v < config::CONFIG_VERSION as u64) {
                // Settings from an older build: persist the upgrade and refresh cost estimates once.
                db.save_config(&config)?;
                if let Err(e) = db.recompute_estimated_costs(&config.pricing) {
                    log::warn!("could not recompute estimated costs: {e}");
                }
            }
            // Older builds stored a pasted key in SQLite: move it to the Keychain and delete the plain copy.
            if let Some(old) = db.setting("deepseekApiKey")? {
                if secrets::set_deepseek_key(Some(&old)).is_ok() {
                    db.set_setting("deepseekApiKey", None)?;
                }
            }
            let (api_key, key_source) = config::resolve_api_key();
            log::info!("DeepSeek key: {}", if api_key.is_some() { key_source } else { "not configured" });
            match db.requeue_transient_failures(api_key.is_some()) {
                Ok(n) if n > 0 => log::info!("re-queued {n} screenshots that failed for temporary reasons"),
                Err(e) => log::warn!("could not re-queue failed screenshots: {e}"),
                _ => {}
            }

            let bridge = Arc::new(NativeBridge::new(NativeBridge::locate_binary()));
            let places = Arc::new(PlaceService::new(bridge.clone()));
            let ai = Arc::new(DeepSeekTravelAIService::new(config.clone(), api_key));
            let fetcher = Arc::new(InstagramFetcher::default());
            let pipeline = Arc::new(Pipeline::new(
                db.clone(), bridge.clone(), bridge.clone(), bridge.clone(), places, bridge.clone(), fetcher, ai, config, data_dir,
            ));
            let notify_handle = app.handle().clone();
            pipeline.set_notifier(Arc::new(move || { let _ = notify_handle.emit("data-changed", ()); }));

            // Resume Reels interrupted by a restart.
            let resume = pipeline.clone();
            tauri::async_runtime::spawn(async move {
                for id in resume.db.reel_ids_needing_processing().unwrap_or_default() {
                    resume.process_reel(&id).await;
                }
            });

            let handle = app.handle().clone();
            let queue = Arc::new(ProcessingQueue::new(pipeline.clone(), Arc::new(move |snapshot| {
                let _ = handle.emit("processing-progress", snapshot);
            })));
            app.manage(AppState { db, pipeline, queue: queue.clone() });

            // Incremental scan whenever Photos reports a change (new screenshots).
            let (handle, observer_queue, observer_bridge) = (app.handle().clone(), queue.clone(), bridge.clone());
            tauri::async_runtime::spawn(async move {
                let status = observer_bridge.authorization_status().await.unwrap_or_default();
                if status == "authorized" || status == "limited" {
                    let _ = observer_bridge.observe_new_screenshots().await;
                    if handle.state::<AppState>().pipeline.config().auto_process_new_screenshots {
                        tauri::async_runtime::spawn(observer_queue.clone().run(RunMode::ScanNew));
                    }
                }
                let mut events = observer_bridge.subscribe();
                while let Ok(event) = events.recv().await {
                    if event == "photosLibraryChanged" {
                        let _ = handle.emit("library-changed", ());
                        // Only when the user turned on automatic processing (off by default).
                        let auto = handle.state::<AppState>().pipeline.config().auto_process_new_screenshots;
                        if auto {
                            tauri::async_runtime::spawn(observer_queue.clone().run(RunMode::ScanNew));
                        }
                    }
                }
            });

            // Pins with no screenshot or Reel behind them go (places you added by hand stay).
            {
                let removed = app.state::<AppState>().db.remove_unsupported_places().unwrap_or(0);
                if removed > 0 { log::info!("removed {removed} places with no screenshot or Reel left"); }
            }
            // Ignored screenshots/Reels never affect places or the map (also for items ignored by older versions).
            {
                let db = app.state::<AppState>().db.clone();
                let ignored: Vec<(Option<String>, Option<String>)> = db.with(|c| {
                    let mut stmt = c.prepare(
                        "SELECT id, NULL FROM screenshots WHERE status = 'ignored' AND id IN (SELECT screenshot_id FROM place_screenshots) \
                         UNION ALL SELECT NULL, id FROM reels WHERE status = 'ignored' AND id IN (SELECT reel_id FROM place_reels)",
                    )?;
                    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
                    rows.collect()
                }).unwrap_or_default();
                for (shot, reel) in ignored {
                    let _ = db.detach_source(shot.as_deref(), reel.as_deref());
                }
            }

            // Work parked while offline (or at the daily AI limit) resumes periodically.
            let retry_queue = queue.clone();
            let retry_state = app.state::<AppState>().db.clone();
            let retry_pipeline = app.state::<AppState>().pipeline.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(300)).await;
                    let waiting = retry_state.screenshot_counts().ok()
                        .and_then(|c| c.get("waitingForNetwork").copied()).unwrap_or(0);
                    if waiting > 0 && !retry_queue.snapshot().running {
                        retry_queue.clone().run(RunMode::Waiting).await;
                    }
                    // Reels parked the same way.
                    for id in retry_state.reel_ids_needing_processing().unwrap_or_default() {
                        retry_pipeline.process_reel(&id).await;
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_overview,
            commands::photos_permission_status,
            commands::request_photos_permission,
            commands::start_processing,
            commands::pause_processing,
            commands::resume_processing,
            commands::cancel_processing,
            commands::processing_status,
            commands::reprocess,
            commands::list_places,
            commands::get_place_detail,
            commands::update_place,
            commands::set_place_location,
            commands::delete_place,
            commands::merge_places,
            commands::split_place,
            commands::remove_place_screenshot,
            commands::delete_fact,
            commands::remove_image,
            commands::summarize_place,
            commands::list_screenshots,
            commands::get_screenshot_detail,
            commands::screenshot_action,
            commands::add_place_to_screenshot,
            commands::correct_place,
            commands::save_manual_crop,
            commands::search_map,
            commands::place_warnings,
            commands::add_manual_place,
            commands::photos_near_place,
            commands::photos_between,
            commands::photo_previews,
            commands::attach_memories,
            commands::remove_memory,
            commands::update_fact,
            commands::add_place_photos,
            commands::add_place_photo_bytes,
            commands::update_image_caption,
            commands::set_cover_source,
            commands::recent_reviews,
            commands::change_review,
            commands::source_reviews,
            commands::count_screenshots,
            commands::screenshot_refs,
            commands::add_fact,
            commands::add_place_to_reel,
            commands::remove_place_from_reel,
            commands::list_reviews,
            commands::resolve_review,
            commands::list_trips,
            commands::create_trip,
            commands::update_trip,
            commands::delete_trip,
            commands::trip_entries,
            commands::add_to_trip,
            commands::update_trip_entry,
            commands::remove_trip_entry,
            commands::get_settings,
            commands::save_settings,
            commands::save_api_key,
            commands::diagnostics,
            commands::cost_summary,
            commands::list_reels,
            commands::get_reel_detail,
            commands::import_reel,
            commands::list_photo_videos,
            commands::import_reel_video_from_photos,
            commands::reel_action,
            commands::reel_tool_status,
            commands::open_external,
            commands::scan_preview,
            commands::add_screenshot_folder,
            commands::remove_screenshot_folder,
            commands::list_runs,
            commands::run_report,
            commands::retranscribe_reel,
            commands::speech_locales,
            commands::backup_library,
            commands::import_reels,
            commands::import_reels_from_file,
            commands::export_places,
            commands::import_screenshot_files,
            commands::reorder_trip,
            commands::export_trip,
            commands::restore_backup,
            commands::restart_app,
            commands::reveal_data_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TravelSnapMap");
}
