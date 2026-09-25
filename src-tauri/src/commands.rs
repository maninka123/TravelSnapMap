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
use crate::pipeline::queue::QueueSnapshot;
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
    let (key, source) = resolve_api_key(state.db.setting("deepseekApiKey").map_err(err)?);
    Ok(json!({
        "screenshotCounts": counts,
        "openReviews": state.db.open_review_count().map_err(err)?,
        "places": state.db.list_places(&PlaceFilter::default()).map_err(err)?.len(),
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

#[tauri::command]
pub async fn start_processing(state: State<'_, AppState>, discover: bool, limit: Option<i64>) -> CmdResult<()> {
    let queue = state.queue.clone();
    tauri::async_runtime::spawn(queue.run(discover, limit));
    Ok(())
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
    tauri::async_runtime::spawn(state.queue.clone().run(false, None));
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
    let (key, source) = resolve_api_key(state.db.setting("deepseekApiKey").map_err(err)?);
    Ok(json!({
        "config": state.pipeline.config(),
        "defaults": AppConfig::default(),
        "hasApiKey": key.is_some(),
        "apiKeySource": source,
    }))
}

fn rebuild_ai(state: &AppState, config: AppConfig) -> CmdResult<()> {
    let (key, _) = resolve_api_key(state.db.setting("deepseekApiKey").map_err(err)?);
    let ai = Arc::new(DeepSeekTravelAIService::new(config.clone(), key));
    state.pipeline.reconfigure(config, ai);
    Ok(())
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, config: AppConfig) -> CmdResult<()> {
    state.db.save_config(&config).map_err(err)?;
    rebuild_ai(&state, config)
}

/// Stores a key in the local app database (outside the repository). `None` removes it.
#[tauri::command]
pub async fn save_api_key(state: State<'_, AppState>, key: Option<String>) -> CmdResult<()> {
    let key = key.map(|k| k.trim().to_string()).filter(|k| !k.is_empty());
    state.db.set_setting("deepseekApiKey", key.as_deref()).map_err(err)?;
    rebuild_ai(&state, state.pipeline.config())
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
        "ignore" => state.db.finish_reel(&id, ProcessingStatus::Ignored, Some("ignored by you")).map_err(err)?,
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
