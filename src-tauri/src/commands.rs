//! Tauri commands: the only API the React UI can call. Thin wrappers over the services —
//! no Apple framework details and no DeepSeek calls leak to the frontend.

use std::sync::Arc;

use serde::Serialize;
use serde_json::{json, Value};
use tauri::State;

use crate::config::{resolve_api_key, AppConfig};
use crate::db::*;
use crate::models::*;
use crate::pipeline::merge;
use crate::pipeline::queue::{QueueSnapshot, RunMode};
use crate::services::ai::deepseek::DeepSeekTravelAIService;
use crate::AppState;

type CmdResult<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// MARK: Overview & processing

#[tauri::command]
pub async fn get_overview(state: State<'_, AppState>) -> CmdResult<Value> {
    let counts = state.db.screenshot_counts().map_err(err)?;
    let config = state.pipeline.config();
    let (key, source) = resolve_api_key();
    Ok(json!({
        "screenshotCounts": counts,
        "openReviews": state.db.open_review_count().map_err(err)?,
        "places": state.db.with(|c| c.query_row("SELECT COUNT(*) FROM places", [], |r| r.get::<_, i64>(0))).map_err(err)?,
        // Unconfirmed places wait in Review and aren't on the map yet.
        "unconfirmedPlaces": state.db.with(|c| c.query_row("SELECT COUNT(*) FROM places WHERE verification = 'needsReview'", [], |r| r.get::<_, i64>(0))).map_err(err)?,
        "queue": state.queue.snapshot(),
        "model": config.model,
        "aiConfigured": key.is_some() || !config.is_direct_deepseek(),
        "apiKeySource": source,
    }))
}

#[tauri::command]
pub async fn photos_permission_status(state: State<'_, AppState>) -> CmdResult<String> {
    state.pipeline.photos.authorization_status().await.map_err(err)
}

#[tauri::command]
pub async fn request_photos_permission(state: State<'_, AppState>) -> CmdResult<String> {
    let status = state.pipeline.photos.request_permission().await.map_err(err)?;
    if status == "authorized" || status == "limited" {
        let _ = state.pipeline.photos.observe_new_screenshots().await;
    }
    Ok(status)
}

/// mode: {"kind": "scanNew"} | {"kind": "validation", "count": 25, "random": true} | {"kind": "retry"}
#[tauri::command]
pub async fn start_processing(state: State<'_, AppState>, mode: RunMode) -> CmdResult<()> {
    let queue = state.queue.clone();
    tauri::async_runtime::spawn(queue.run(mode));
    Ok(())
}

/// How many screenshots each source has, how many are already known, and how many are new
/// (PhotoKit asset IDs / file paths compared with the database).
#[tauri::command]
pub async fn scan_preview(state: State<'_, AppState>) -> CmdResult<Value> {
    let config = state.pipeline.config();
    let known = state.db.known_photo_ids().map_err(err)?;
    let count = |assets: &[crate::services::native::AssetInfo]| {
        let already = assets.iter().filter(|a| known.contains(&a.id)).count();
        json!({ "total": assets.len(), "known": already, "new": assets.len() - already })
    };
    let photos = match state.pipeline.photos.list_screenshots(config.scan_from_year, config.scan_to_year).await {
        Ok(a) => count(&a),
        Err(e) => json!({ "error": e.to_string() }),
    };
    let mut folders = Vec::new();
    for f in &config.screenshot_folders {
        let v = match crate::services::folder::scan_folder(std::path::Path::new(f)) {
            Ok(a) => count(&a),
            Err(e) => json!({ "error": e.to_string() }),
        };
        folders.push(json!({ "path": f, "summary": v }));
    }
    Ok(json!({ "photos": photos, "folders": folders }))
}

/// Adds a folder of screenshots (from any device) and scans its new images.
#[tauri::command]
pub async fn add_screenshot_folder(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let dir = std::path::Path::new(&path);
    if !dir.is_dir() {
        return Err("That folder doesn't exist.".into());
    }
    let mut config = state.pipeline.config();
    if !config.screenshot_folders.contains(&path) {
        config.screenshot_folders.push(path.clone());
        state.db.save_config(&config).map_err(err)?;
        rebuild_ai(&state, config)?;
    }
    tauri::async_runtime::spawn(state.queue.clone().run(RunMode::ScanFolder { path }));
    Ok(())
}

/// Stops scanning a folder (screenshots already imported stay in the library).
#[tauri::command]
pub async fn remove_screenshot_folder(state: State<'_, AppState>, path: String) -> CmdResult<()> {
    let mut config = state.pipeline.config();
    config.screenshot_folders.retain(|f| f != &path);
    state.db.save_config(&config).map_err(err)?;
    rebuild_ai(&state, config)
}

#[tauri::command]
pub async fn list_runs(state: State<'_, AppState>) -> CmdResult<Vec<RunRecord>> {
    state.db.runs().map_err(err)
}

#[tauri::command]
pub async fn run_report(state: State<'_, AppState>, id: String) -> CmdResult<Option<RunReport>> {
    state.db.run_report(&id).map_err(err)
}

#[tauri::command]
pub async fn pause_processing(state: State<'_, AppState>) -> CmdResult<()> {
    state.queue.pause();
    Ok(())
}

#[tauri::command]
pub async fn resume_processing(state: State<'_, AppState>) -> CmdResult<()> {
    state.queue.resume();
    Ok(())
}

#[tauri::command]
pub async fn cancel_processing(state: State<'_, AppState>) -> CmdResult<()> {
    state.queue.cancel();
    Ok(())
}

#[tauri::command]
pub async fn processing_status(state: State<'_, AppState>) -> CmdResult<QueueSnapshot> {
    Ok(state.queue.snapshot())
}

/// scope: failed | needsReview | outdated | all | selected
#[tauri::command]
pub async fn reprocess(state: State<'_, AppState>, scope: String, ids: Option<Vec<String>>) -> CmdResult<usize> {
    let targets = state.db.mark_for_reprocess(&scope, &ids.unwrap_or_default()).map_err(err)?;
    tauri::async_runtime::spawn(state.queue.clone().run(RunMode::Retry));
    Ok(targets.len())
}

// MARK: Places

#[tauri::command]
pub async fn list_places(state: State<'_, AppState>, filter: Option<PlaceFilter>) -> CmdResult<Vec<PlaceRecord>> {
    state.db.list_places(&filter.unwrap_or_default()).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NearbyPlace {
    place: PlaceRecord,
    distance_km: f64,
}

#[tauri::command]
pub async fn get_place_detail(state: State<'_, AppState>, id: String) -> CmdResult<Value> {
    let db = &state.db;
    let place = db.place(&id).map_err(err)?.ok_or("Place not found")?;
    let radius = state.pipeline.config().nearby_radius_km;
    let delta = radius / 111.0 * 1.5;
    let mut nearby: Vec<NearbyPlace> = db
        .places_near(place.latitude, place.longitude, delta)
        .map_err(err)?
        .into_iter()
        .filter(|p| p.id != place.id)
        .map(|p| {
            let d = crate::text::distance_m(place.latitude, place.longitude, p.latitude, p.longitude) / 1000.0;
            NearbyPlace { place: p, distance_km: d }
        })
        .filter(|n| n.distance_km <= radius)
        .collect();
    nearby.sort_by(|a, b| a.distance_km.total_cmp(&b.distance_km));
    nearby.truncate(12);
    Ok(json!({
        "place": place,
        "facts": db.facts_for_place(&id).map_err(err)?,
        "screenshots": db.screenshots_for_place(&id).map_err(err)?,
        "reels": db.reels_for_place(&id).map_err(err)?,
        "links": db.links_for_place(&id).map_err(err)?,
        "images": db.images_for_place(&id).map_err(err)?,
        "nearby": nearby,
        "trips": db.trips_for_place(&id).map_err(err)?,
        "memories": db.memories_for_place(&id).map_err(err)?,
        "warnings": crate::insights::warnings_for(&db.facts_for_place(&id).map_err(err)?, place.category, chrono::Local::now().date_naive()),
    }))
}

/// Editable fields: canonicalName, category, personalStatus, notes, heroImageId.
/// Name/category edits mark the place user-verified so automation never overwrites them.
#[tauri::command]
pub async fn update_place(state: State<'_, AppState>, id: String, field: String, value: Option<String>) -> CmdResult<()> {
    state.db.update_place_field(&id, &field, value.as_deref()).map_err(err)?;
    if matches!(field.as_str(), "canonicalName" | "category") {
        state.db.set_place_user_verified(&id).map_err(err)?;
    }
    Ok(())
}

#[tauri::command]
pub async fn set_place_location(state: State<'_, AppState>, id: String, candidate: PlaceCandidate) -> CmdResult<()> {
    state.db.update_place_location(&id, &candidate, Verification::UserVerified).map_err(err)?;
    state.db.set_place_user_verified(&id).map_err(err)
}

#[tauri::command]
pub async fn delete_place(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.delete_place(&id).map_err(err)
}

#[tauri::command]
pub async fn merge_places(state: State<'_, AppState>, source_id: String, target_id: String) -> CmdResult<()> {
    merge::merge_places(&state.db, &source_id, &target_id).map_err(err)
}

#[tauri::command]
pub async fn split_place(state: State<'_, AppState>, place_id: String, screenshot_ids: Vec<String>, candidate: PlaceCandidate) -> CmdResult<String> {
    merge::split_place(&state.db, &place_id, &screenshot_ids, &candidate).map_err(err)
}

#[tauri::command]
pub async fn remove_place_screenshot(state: State<'_, AppState>, place_id: String, screenshot_id: String) -> CmdResult<()> {
    state.db.unlink(&place_id, &screenshot_id).map_err(err)
}

#[tauri::command]
pub async fn delete_fact(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.delete_fact(&id).map_err(err)
}

#[tauri::command]
pub async fn remove_image(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    if let Some(img) = state.db.image(&id).map_err(err)? {
        if let Some(p) = img.image_path {
            let _ = std::fs::remove_file(p);
        }
    }
    state.db.delete_image(&id).map_err(err)
}

#[tauri::command]
pub async fn summarize_place(state: State<'_, AppState>, id: String) -> CmdResult<String> {
    state.pipeline.summarize_place(&id).await.map_err(err)
}

// MARK: Screenshots

#[tauri::command]
pub async fn list_screenshots(state: State<'_, AppState>, filter: Option<ScreenshotFilter>) -> CmdResult<Vec<ScreenshotRecord>> {
    state.db.list_screenshots(&filter.unwrap_or_default()).map_err(err)
}

#[tauri::command]
pub async fn get_screenshot_detail(state: State<'_, AppState>, id: String) -> CmdResult<Value> {
    let db = &state.db;
    let shot = db.screenshot(&id).map_err(err)?.ok_or("Screenshot not found")?;
    Ok(json!({
        "screenshot": shot,
        "blocks": db.ocr_blocks(&id).map_err(err)?,
        "places": db.places_for_screenshot(&id).map_err(err)?,
        "links": db.links_for_screenshot(&id).map_err(err)?,
        "facts": db.facts_for_screenshot(&id).map_err(err)?,
        "images": db.images_for_screenshot(&id).map_err(err)?,
        "reviews": db.reviews_for_screenshot(&id).map_err(err)?,
    }))
}

/// action: reprocess | ignore | markTravel | markNotTravel
#[tauri::command]
pub async fn screenshot_action(state: State<'_, AppState>, id: String, action: String) -> CmdResult<()> {
    let p = &state.pipeline;
    match action.as_str() {
        "reprocess" => { p.reprocess(&id).await.map_err(err)?; }
        "ignore" => p.ignore(&id).map_err(err)?,
        "markTravel" => { p.confirm_travel(&id).await.map_err(err)?; }
        "markNotTravel" => p.mark_not_travel(&id).map_err(err)?,
        other => return Err(format!("unknown action {other}")),
    }
    Ok(())
}

#[tauri::command]
pub async fn add_place_to_screenshot(state: State<'_, AppState>, screenshot_id: String, candidate: PlaceCandidate) -> CmdResult<String> {
    state.pipeline.user_attach(&screenshot_id, &candidate, None).map_err(err)
}

#[tauri::command]
pub async fn correct_place(state: State<'_, AppState>, screenshot_id: String, wrong_place_id: String, candidate: PlaceCandidate) -> CmdResult<String> {
    state.pipeline.correct_place(&screenshot_id, &wrong_place_id, &candidate).map_err(err)
}

#[tauri::command]
pub async fn save_manual_crop(state: State<'_, AppState>, screenshot_id: String, place_id: String, rect: Rect) -> CmdResult<String> {
    state.pipeline.save_manual_crop(&screenshot_id, &place_id, rect).await.map_err(err)
}

/// Map search for manual corrections (the provider, never the AI, supplies coordinates).
#[tauri::command]
pub async fn search_map(state: State<'_, AppState>, query: String) -> CmdResult<Vec<PlaceCandidate>> {
    state.pipeline.places.search(&query).await.map_err(err)
}

// MARK: Review inbox

#[tauri::command]
pub async fn list_reviews(state: State<'_, AppState>) -> CmdResult<Value> {
    let reviews = state.db.open_reviews().map_err(err)?;
    let mut out = Vec::new();
    for r in reviews {
        let a = r.place_a_id.as_deref().map(|id| state.db.place(id)).transpose().map_err(err)?.flatten();
        let b = r.place_b_id.as_deref().map(|id| state.db.place(id)).transpose().map_err(err)?.flatten();
        let img = r.image_id.as_deref().map(|id| state.db.image(id)).transpose().map_err(err)?.flatten();
        out.push(json!({ "review": r, "placeA": a, "placeB": b, "image": img }));
    }
    Ok(Value::Array(out))
}

#[tauri::command]
pub async fn resolve_review(state: State<'_, AppState>, id: String, action: String, candidate: Option<PlaceCandidate>) -> CmdResult<()> {
    state.pipeline.resolve_review(&id, &action, candidate).await.map_err(err)
}

// MARK: Trips

#[tauri::command]
pub async fn list_trips(state: State<'_, AppState>) -> CmdResult<Vec<TripRecord>> {
    state.db.list_trips().map_err(err)
}

#[tauri::command]
pub async fn create_trip(state: State<'_, AppState>, name: String) -> CmdResult<String> {
    state.db.create_trip(name.trim()).map_err(err)
}

#[tauri::command]
pub async fn update_trip(state: State<'_, AppState>, id: String, name: String, start_date: Option<String>, end_date: Option<String>, notes: String) -> CmdResult<()> {
    state.db.update_trip(&id, &name, start_date.as_deref(), end_date.as_deref(), &notes).map_err(err)
}

#[tauri::command]
pub async fn delete_trip(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.db.delete_trip(&id).map_err(err)
}

#[tauri::command]
pub async fn trip_entries(state: State<'_, AppState>, trip_id: String) -> CmdResult<Vec<TripEntryRecord>> {
    state.db.trip_entries(&trip_id).map_err(err)
}

#[tauri::command]
pub async fn add_to_trip(state: State<'_, AppState>, trip_id: String, place_id: String) -> CmdResult<()> {
    state.db.add_to_trip(&trip_id, &place_id).map_err(err)
}

#[tauri::command]
pub async fn update_trip_entry(state: State<'_, AppState>, entry_id: String, day: Option<i64>, position: Option<i64>) -> CmdResult<()> {
    state.db.update_trip_entry(&entry_id, day, position).map_err(err)
}

#[tauri::command]
pub async fn remove_trip_entry(state: State<'_, AppState>, entry_id: String) -> CmdResult<()> {
    state.db.remove_trip_entry(&entry_id).map_err(err)
}

// MARK: Settings & diagnostics

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Value> {
    let (key, source) = resolve_api_key();
    Ok(json!({
        "config": state.pipeline.config(),
        "defaults": AppConfig::default(),
        "hasApiKey": key.is_some(),
        "apiKeySource": source,
    }))
}

fn rebuild_ai(state: &AppState, config: AppConfig) -> CmdResult<()> {
    let (key, _) = resolve_api_key();
    let ai = Arc::new(DeepSeekTravelAIService::new(config.clone(), key));
    state.pipeline.reconfigure(config, ai);
    Ok(())
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, config: AppConfig) -> CmdResult<()> {
    let pricing_changed = state.pipeline.config().pricing != config.pricing;
    state.db.save_config(&config).map_err(err)?;
    if pricing_changed {
        // Estimates follow the pricing settings; token counts themselves never change.
        state.db.recompute_estimated_costs(&config.pricing).map_err(err)?;
    }
    rebuild_ai(&state, config)
}

/// Stores the key in the macOS Keychain. `None` removes it.
#[tauri::command]
pub async fn save_api_key(state: State<'_, AppState>, key: Option<String>) -> CmdResult<()> {
    crate::secrets::set_deepseek_key(key.as_deref()).map_err(err)?;
    rebuild_ai(&state, state.pipeline.config())
}

/// Estimated spend so far under `pricing` (the Settings form, saved or not), with each mode for comparison.
#[tauri::command]
pub async fn cost_summary(state: State<'_, AppState>, pricing: crate::config::Pricing) -> CmdResult<crate::db::CostSummary> {
    state.db.cost_summary(&pricing).map_err(err)
}

#[tauri::command]
pub async fn diagnostics(state: State<'_, AppState>) -> CmdResult<Diagnostics> {
    state.db.diagnostics().map_err(err)
}

// MARK: Instagram Reels

#[tauri::command]
pub async fn list_reels(state: State<'_, AppState>) -> CmdResult<Vec<ReelRecord>> {
    state.db.list_reels().map_err(err)
}

#[tauri::command]
pub async fn get_reel_detail(state: State<'_, AppState>, id: String) -> CmdResult<Value> {
    let db = &state.db;
    let reel = db.reel(&id).map_err(err)?.ok_or("Reel not found")?;
    Ok(json!({
        "reel": reel,
        "keyframes": db.keyframes(&id).map_err(err)?,
        "places": db.places_for_reel(&id).map_err(err)?,
        "facts": db.facts_for_reel(&id).map_err(err)?,
        "images": db.images_for_reel(&id).map_err(err)?,
        "reviews": db.reviews_for_reel(&id).map_err(err)?,
    }))
}

fn spawn_reel(state: &AppState, id: String, reprocess: bool) {
    let pipeline = state.pipeline.clone();
    tauri::async_runtime::spawn(async move {
        if reprocess { pipeline.reprocess_reel(&id).await } else { pipeline.process_reel(&id).await };
    });
}

/// Paste a Reel/Post URL: saved immediately, processed in the background.
#[tauri::command]
pub async fn import_reel(state: State<'_, AppState>, url: String) -> CmdResult<String> {
    let (id, created) = state.pipeline.register_reel(&url).map_err(err)?;
    let status = state.db.reel(&id).map_err(err)?.map(|r| r.status);
    if created || status.is_some_and(|s| s.needs_processing()) {
        spawn_reel(&state, id.clone(), false);
    }
    Ok(id)
}

#[tauri::command]
pub async fn list_photo_videos(state: State<'_, AppState>) -> CmdResult<Vec<crate::services::native::PhotoVideo>> {
    state.pipeline.media.list_videos(120).await.map_err(err)
}

#[tauri::command]
pub async fn import_reel_video_from_photos(state: State<'_, AppState>, reel_id: Option<String>, asset_id: String) -> CmdResult<String> {
    let id = state.pipeline.attach_photos_video(reel_id.as_deref(), &asset_id).await.map_err(err)?;
    spawn_reel(&state, id.clone(), false);
    Ok(id)
}

/// action: reprocess | ignore | delete
#[tauri::command]
pub async fn reel_action(state: State<'_, AppState>, id: String, action: String) -> CmdResult<()> {
    match action.as_str() {
        "reprocess" => spawn_reel(&state, id, true),
        "ignore" => {
            // Like screenshots: it stays under "Ignored" but no longer affects places, the map or Review.
            state.db.detach_source(None, Some(&id)).map_err(err)?;
            state.db.finish_reel(&id, ProcessingStatus::Ignored, Some("ignored by you")).map_err(err)?;
        }
        "delete" => {
            state.db.delete_reel(&id).map_err(err)?;
            let _ = std::fs::remove_dir_all(state.pipeline.data_dir.join("reels").join(&id));
        }
        other => return Err(format!("unknown action {other}")),
    }
    Ok(())
}

#[tauri::command]
pub async fn reel_tool_status(state: State<'_, AppState>) -> CmdResult<Value> {
    let path = state.pipeline.fetcher.yt_dlp(&state.pipeline.config());
    Ok(json!({ "ytDlp": path.map(|p| p.to_string_lossy().to_string()) }))
}

/// Opens an http(s) link in the default browser.
#[tauri::command]
pub async fn open_external(url: String) -> CmdResult<()> {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err("Only web links can be opened".into());
    }
    std::process::Command::new("open").arg(&url).spawn().map_err(err)?;
    Ok(())
}

/// Re-transcribe a Reel with a chosen language ("auto" to detect again) and update its places.
#[tauri::command]
pub async fn retranscribe_reel(state: State<'_, AppState>, id: String, locale: String) -> CmdResult<()> {
    let pipeline = state.pipeline.clone();
    tauri::async_runtime::spawn(async move { pipeline.retranscribe_reel(&id, &locale).await });
    Ok(())
}

/// Speech locales Apple supports on this Mac (and whether they run on-device).
#[tauri::command]
pub async fn speech_locales(state: State<'_, AppState>) -> CmdResult<Vec<crate::services::native::SpeechLocale>> {
    state.pipeline.media.speech_locales().await.map_err(err)
}

// MARK: Backup & export

/// Writes a .zip backup (database, place photos/crops, Reel audio/frames, settings) — never the API key.
#[tauri::command]
pub async fn backup_library(state: State<'_, AppState>, path: String) -> CmdResult<crate::backup::BackupSummary> {
    let (db, data_dir, config) = (state.db.clone(), state.pipeline.data_dir.clone(), state.pipeline.config());
    tauri::async_runtime::spawn_blocking(move || crate::backup::create_backup(&db, &data_dir, &config, std::path::Path::new(&path)))
        .await
        .map_err(err)?
        .map_err(err)
}

/// format: "json" (all places with facts and sources) or "geojson" (verified places).
#[tauri::command]
pub async fn export_places(state: State<'_, AppState>, path: String, format: String) -> CmdResult<usize> {
    let value = match format.as_str() {
        "geojson" => crate::backup::places_geojson(&state.db),
        _ => crate::backup::places_json(&state.db),
    }
    .map_err(err)?;
    let count = value["features"].as_array().or(value["places"].as_array()).map(|a| a.len()).unwrap_or(0);
    crate::backup::write_json(&value, std::path::Path::new(&path)).map_err(err)?;
    Ok(count)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkReelImport {
    /// Instagram links found in the text.
    found: usize,
    /// New Reels added and queued.
    added: usize,
    /// Links that were already in the library.
    already_imported: usize,
    ids: Vec<String>,
}

/// One worker processes bulk imports sequentially (kind to Instagram's rate limits).
static REEL_WORKER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn import_many(state: &AppState, text: &str) -> CmdResult<BulkReelImport> {
    let urls = crate::services::reels::extract_instagram_urls(text);
    let (mut ids, mut added, mut already) = (Vec::new(), 0, 0);
    for url in &urls {
        let (id, created) = state.pipeline.register_reel(url).map_err(err)?;
        if created { added += 1 } else { already += 1 }
        let pending = state.db.reel(&id).map_err(err)?.is_some_and(|r| r.status.needs_processing());
        if created || pending {
            ids.push(id);
        }
    }
    let pipeline = state.pipeline.clone();
    let queue = ids.clone();
    tauri::async_runtime::spawn(async move {
        let _guard = REEL_WORKER.lock().await;
        for (i, id) in queue.iter().enumerate() {
            if i > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
            pipeline.process_reel(id).await;
        }
    });
    Ok(BulkReelImport { found: urls.len(), added, already_imported: already, ids })
}

/// Many links at once (pasted list, one per line or mixed with other text).
#[tauri::command]
pub async fn import_reels(state: State<'_, AppState>, text: String) -> CmdResult<BulkReelImport> {
    import_many(&state, &text)
}

/// All Instagram links inside a text file (.txt, .md, .csv, notes export…).
#[tauri::command]
pub async fn import_reels_from_file(state: State<'_, AppState>, path: String) -> CmdResult<BulkReelImport> {
    let meta = std::fs::metadata(&path).map_err(err)?;
    if meta.len() > 10_000_000 {
        return Err("That file is too large (over 10 MB) — is it really a list of links?".into());
    }
    let bytes = std::fs::read(&path).map_err(err)?;
    import_many(&state, &String::from_utf8_lossy(&bytes))
}

// MARK: Quick add, warnings and after-trip memories

/// Freshness and conflict warnings for every place that has any.
#[tauri::command]
pub async fn place_warnings(state: State<'_, AppState>) -> CmdResult<std::collections::HashMap<String, Vec<crate::insights::PlaceWarning>>> {
    crate::insights::all_warnings(&state.db).map_err(err)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualPlace {
    id: String,
    /// The place was already saved (same Apple Maps place or same name nearby); nothing was duplicated.
    existing: bool,
}

/// + Add Place: a place you know without a screenshot or Reel. Coordinates come from Apple Maps.
#[tauri::command]
pub async fn add_manual_place(
    state: State<'_, AppState>,
    candidate: PlaceCandidate,
    status: String,
    category: Option<String>,
    notes: Option<String>,
) -> CmdResult<ManualPlace> {
    let db = &state.db;
    if let merge::PlaceMatch::Same(existing) = merge::find_match(db, &candidate, &[]).map_err(err)? {
        return Ok(ManualPlace { id: existing.id, existing: true });
    }
    let category = category
        .as_deref()
        .and_then(PlaceCategory::try_parse)
        .or_else(|| candidate.category_hint())
        .unwrap_or_default();
    let place = db.insert_place(&candidate, category, Verification::UserVerified, DataOrigin::User, &[]).map_err(err)?;
    let status = PersonalStatus::try_parse(&status).unwrap_or_default();
    db.update_place_field(&place.id, "personalStatus", Some(status.as_str())).map_err(err)?;
    if let Some(n) = notes.filter(|n| !n.trim().is_empty()) {
        db.update_place_field(&place.id, "notes", Some(n.trim())).map_err(err)?;
    }
    Ok(ManualPlace { id: place.id, existing: false })
}

/// Your own photos (not screenshots) taken within `radius_km` of the place.
#[tauri::command]
pub async fn photos_near_place(state: State<'_, AppState>, place_id: String, radius_km: f64) -> CmdResult<Vec<crate::services::native::OwnPhoto>> {
    let place = state.db.place(&place_id).map_err(err)?.ok_or("Place not found")?;
    state.pipeline.photos.photos_near(place.latitude, place.longitude, radius_km.clamp(0.05, 50.0), 400).await.map_err(err)
}

/// Your own photos taken between two days (YYYY-MM-DD, inclusive).
#[tauri::command]
pub async fn photos_between(state: State<'_, AppState>, from: String, to: String) -> CmdResult<Vec<crate::services::native::OwnPhoto>> {
    state.pipeline.photos.photos_between(&from, &to, 400).await.map_err(err)
}

fn preview_name(asset_id: &str) -> String {
    format!("{}.jpg", asset_id.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect::<String>())
}

/// Small previews for the photo picker (cached). Returns asset id → file path; unavailable ones are left out.
#[tauri::command]
pub async fn photo_previews(state: State<'_, AppState>, ids: Vec<String>) -> CmdResult<std::collections::HashMap<String, String>> {
    use futures::stream::{self, StreamExt};
    let dir = state.pipeline.data_dir.join("memories").join(".previews");
    std::fs::create_dir_all(&dir).map_err(err)?;
    let photos = state.pipeline.photos.clone();
    let results: Vec<Option<(String, String)>> = stream::iter(ids)
        .map(|id| {
            let photos = photos.clone();
            let path = dir.join(preview_name(&id));
            async move {
                if !path.exists() {
                    photos.thumbnail(&id, &path, 360).await.ok()?;
                }
                Some((id, path.to_string_lossy().into_owned()))
            }
        })
        .buffer_unordered(6)
        .collect()
        .await;
    Ok(results.into_iter().flatten().collect())
}

/// Attaches your own photos to a place (copies are made; originals stay in Photos) and marks it visited.
#[tauri::command]
pub async fn attach_memories(state: State<'_, AppState>, place_id: String, photos: Vec<crate::services::native::OwnPhoto>) -> CmdResult<usize> {
    let db = &state.db;
    db.place(&place_id).map_err(err)?.ok_or("Place not found")?;
    let dir = state.pipeline.data_dir.join("memories").join(&place_id);
    std::fs::create_dir_all(&dir).map_err(err)?;
    let mut added = 0;
    let mut last_error = None;
    for photo in &photos {
        if db.memory_exists(&place_id, &photo.id).map_err(err)? {
            continue;
        }
        let name = new_id();
        let image = dir.join(format!("{name}.jpg"));
        let thumb = dir.join(format!("{name}_thumb.jpg"));
        let exported = async {
            state.pipeline.photos.export_image(&photo.id, &image, 2048).await?;
            state.pipeline.photos.thumbnail(&photo.id, &thumb, 480).await
        }
        .await;
        if let Err(e) = exported {
            last_error = Some(e.to_string());
            let _ = std::fs::remove_file(&image);
            continue;
        }
        let memory = NewMemory {
            place_id: &place_id,
            photos_id: &photo.id,
            taken_at: photo.creation_date.as_deref(),
            latitude: photo.latitude,
            longitude: photo.longitude,
            image_path: &image.to_string_lossy(),
            thumbnail_path: &thumb.to_string_lossy(),
        };
        if db.insert_memory(&memory).map_err(err)?.is_some() {
            added += 1;
        }
    }
    if added == 0 {
        if let Some(e) = last_error {
            return Err(format!("Couldn't copy the photos from Photos: {e}"));
        }
    }
    let first_day = photos.iter().filter_map(|p| p.creation_date.as_deref()).filter_map(|d| d.get(..10)).min().map(String::from);
    db.mark_visited(&place_id, first_day.as_deref()).map_err(err)?;
    Ok(added)
}

#[tauri::command]
pub async fn remove_memory(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    if let Some(m) = state.db.memory(&id).map_err(err)? {
        for p in [m.image_path, m.thumbnail_path].into_iter().flatten() {
            let _ = std::fs::remove_file(p);
        }
    }
    state.db.delete_memory(&id).map_err(err)
}

/// How many screenshots a Sources view holds (for tab counts and "Showing 300 of 1,421").
#[tauri::command]
pub async fn count_screenshots(state: State<'_, AppState>, filter: Option<ScreenshotFilter>) -> CmdResult<i64> {
    state.db.count_screenshots(&filter.unwrap_or_default()).map_err(err)
}

/// All screenshots in a Sources view as {id, date}, in list order (for the viewer's ‹ › across the whole tab).
#[tauri::command]
pub async fn screenshot_refs(state: State<'_, AppState>, filter: Option<ScreenshotFilter>) -> CmdResult<Vec<Value>> {
    Ok(state.db.screenshot_refs(&filter.unwrap_or_default()).map_err(err)?
        .into_iter().map(|(id, date)| json!({"id": id, "date": date})).collect())
}

// MARK: Editing processed results (your edits are kept when something is reprocessed)

/// Edits a saved tip/fact. It becomes yours (origin "user"), so reprocessing never overwrites it.
#[tauri::command]
pub async fn update_fact(state: State<'_, AppState>, id: String, text: String, fact_type: String) -> CmdResult<()> {
    let text = text.trim();
    if text.is_empty() {
        return Err("A tip can't be empty — delete it instead.".into());
    }
    let kind = TravelFactType::try_parse(&fact_type).unwrap_or_default();
    state.db.with(|c| c.execute(
        "UPDATE travel_facts SET text = ?2, type = ?3, origin = 'user', confidence = 1.0 WHERE id = ?1",
        rusqlite::params![id, text, kind],
    )).map_err(err)?;
    Ok(())
}

/// Adds your own tip to a place, optionally tied to the screenshot or Reel you're looking at.
#[tauri::command]
pub async fn add_fact(
    state: State<'_, AppState>,
    place_id: String,
    fact_type: String,
    text: String,
    screenshot_id: Option<String>,
    reel_id: Option<String>,
) -> CmdResult<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Write the tip first.".into());
    }
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let source = FactProvenance {
        screenshot_id: screenshot_id.as_deref(),
        reel_id: reel_id.as_deref(),
        kind: "user",
        time_sec: None,
        block_ids: vec![],
        valid_from: Some(&today),
    };
    let kind = TravelFactType::try_parse(&fact_type).unwrap_or_default();
    state.db.insert_fact(&place_id, kind, text, None, 1.0, &source, DataOrigin::User).map_err(err)
}

/// Links a place (an existing saved one, or a new one from Apple Maps) to a Reel.
#[tauri::command]
pub async fn add_place_to_reel(state: State<'_, AppState>, reel_id: String, candidate: PlaceCandidate) -> CmdResult<String> {
    let db = &state.db;
    let place_id = match merge::find_match(db, &candidate, &[]).map_err(err)? {
        merge::PlaceMatch::Same(p) => p.id,
        _ => db.insert_place(&candidate, candidate.category_hint().unwrap_or_default(), Verification::UserVerified, DataOrigin::User, &[])
            .map_err(err)?.id,
    };
    db.link_reel(&place_id, &reel_id, &candidate.name, 1.0, DataOrigin::User).map_err(err)?;
    db.set_place_user_verified(&place_id).map_err(err)?;
    Ok(place_id)
}

/// Removes a place from a Reel (the place stays if it has other sources or your own data).
#[tauri::command]
pub async fn remove_place_from_reel(state: State<'_, AppState>, place_id: String, reel_id: String) -> CmdResult<()> {
    state.db.unlink_reel(&place_id, &reel_id).map_err(err)
}
