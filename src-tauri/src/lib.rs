//! TravelSnapMap backend: Tauri app setup and dependency wiring.

mod commands;
mod config;
mod countries;
mod db;
mod models;
mod pipeline;
mod services;
mod text;

#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::db::Database;
use crate::pipeline::queue::ProcessingQueue;
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
}

pub fn run() {
    load_env();
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();

    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let db = Arc::new(Database::open(&data_dir.join("travelsnapmap.sqlite"))?);
            let config = db.config();
            let (api_key, _) = config::resolve_api_key(db.setting("deepseekApiKey")?);

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
                }
                let mut events = observer_bridge.subscribe();
                while let Ok(event) = events.recv().await {
                    if event == "photosLibraryChanged" {
                        let _ = handle.emit("library-changed", ());
                        tauri::async_runtime::spawn(observer_queue.clone().run(true, None));
                    }
                }
            });

            // Work parked while offline (or at the daily AI limit) resumes periodically.
            let retry_queue = queue.clone();
            let retry_state = app.state::<AppState>().db.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(300)).await;
                    let waiting = retry_state.screenshot_counts().ok()
                        .and_then(|c| c.get("waitingForNetwork").copied()).unwrap_or(0);
                    if waiting > 0 && !retry_queue.snapshot().running {
                        retry_queue.clone().run(false, None).await;
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
            commands::list_reels,
            commands::get_reel_detail,
            commands::import_reel,
            commands::list_photo_videos,
            commands::import_reel_video_from_photos,
            commands::reel_action,
            commands::reel_tool_status,
            commands::open_external,
        ])
        .run(tauri::generate_context!())
        .expect("error while running TravelSnapMap");
}
