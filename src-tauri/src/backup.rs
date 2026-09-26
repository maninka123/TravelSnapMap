//! Backup and export of the user's library. Backups never contain the DeepSeek API key or any
//! Keychain secret (the key lives only in the Keychain / environment; secret-like settings are also
//! stripped from the database copy as a safety net).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};

use crate::config::AppConfig;
use crate::db::{Database, PlaceFilter};
use crate::models::Verification;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupSummary {
    pub path: String,
    pub size_bytes: u64,
    pub places: usize,
    pub screenshots: i64,
    pub reels: usize,
    pub files: usize,
}

/// Folders inside the app data directory that hold irreplaceable-or-slow-to-rebuild files.
const INCLUDED_DIRS: &[&str] = &["crops", "thumbnails", "reels", "memories"];
/// Large files that can be re-downloaded or regenerated (Reel videos; the full-size screenshot cache).
const EXCLUDED_FILE_NAMES: &[&str] = &["video.mp4"];

/// Creates `dest` (a .zip) with the database snapshot, place photos/crops, thumbnails, Reel audio/frames,
/// the app configuration and restore instructions.
pub fn create_backup(db: &Database, data_dir: &Path, config: &AppConfig, dest: &Path) -> Result<BackupSummary> {
    let stamp = chrono::Local::now().format("%Y-%m-%d %H%M");
    let staging_root = data_dir.join("backups").join(format!(".staging-{}", uuid::Uuid::new_v4()));
    let folder_name = format!("TravelSnapMap Backup {stamp}");
    let staging = staging_root.join(&folder_name);
    std::fs::create_dir_all(&staging)?;
    let result = (|| -> Result<BackupSummary> {
        // 1. Consistent database snapshot, then strip anything secret-like.
        let db_copy = staging.join("travelsnapmap.sqlite");
        db.snapshot_to(&db_copy)?;
        strip_secrets(&db_copy)?;

        // 2. Files.
        let mut files = 0;
        for dir in INCLUDED_DIRS {
            let src = data_dir.join(dir);
            if src.is_dir() {
                files += copy_tree(&src, &staging.join("files").join(dir))?;
            }
        }

        // 3. Configuration (no secrets in AppConfig) and restore instructions.
        std::fs::write(staging.join("config.json"), serde_json::to_string_pretty(config)?)?;
        std::fs::write(staging.join("README.txt"), restore_instructions(data_dir))?;

        // 4. One .zip (ditto ships with macOS and preserves names/metadata).
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let _ = std::fs::remove_file(dest);
        let status = std::process::Command::new("ditto")
            .args(["-c", "-k", "--sequesterRsrc", "--keepParent"])
            .arg(&staging)
            .arg(dest)
            .status()
            .context("running ditto")?;
        anyhow::ensure!(status.success(), "creating the zip failed");

        let places = db.list_places(&PlaceFilter::default())?.len();
        let screenshots: i64 = db.with(|c| c.query_row("SELECT COUNT(*) FROM screenshots", [], |r| r.get(0)))?;
        Ok(BackupSummary {
            path: dest.to_string_lossy().to_string(),
            size_bytes: std::fs::metadata(dest)?.len(),
            places,
            screenshots,
            reels: db.list_reels()?.len(),
            files,
        })
    })();
    let _ = std::fs::remove_dir_all(&staging_root);
    result
}

fn strip_secrets(db_copy: &Path) -> Result<()> {
    let conn = rusqlite::Connection::open(db_copy)?;
    conn.execute(
        "DELETE FROM settings WHERE key = 'deepseekApiKey' OR lower(key) LIKE '%key%' OR lower(key) LIKE '%secret%' \
         OR lower(key) LIKE '%token%' OR lower(key) LIKE '%password%'",
        [],
    )?;
    conn.execute_batch("VACUUM")?; // make sure deleted values don't linger in free pages
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path) -> Result<usize> {
    let mut count = 0;
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let path = entry.path();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        if path.is_dir() {
            count += copy_tree(&path, &dst.join(&name))?;
        } else if !EXCLUDED_FILE_NAMES.contains(&name.to_string_lossy().as_ref()) {
            std::fs::copy(&path, dst.join(&name))?;
            count += 1;
        }
    }
    Ok(count)
}

fn restore_instructions(data_dir: &Path) -> String {
    format!(
        "TravelSnapMap backup\n====================\n\n\
         Contents\n  travelsnapmap.sqlite  your library (places, facts, screenshots, Reels, trips, settings)\n\
         \x20 files/crops           place photos cut from screenshots\n  files/thumbnails      screenshot thumbnails\n\
         \x20 files/memories        your own photos attached to visited places\n\
         \x20 files/reels           Reel audio, key snapshots and covers (videos are not included)\n\
         \x20 config.json           app settings\n\n\
         Not included: your DeepSeek API key (it stays in the macOS Keychain), Reel videos and the full-size\n\
         screenshot cache (both can be re-downloaded or regenerated).\n\n\
         Restore\n  1. Quit TravelSnapMap.\n  2. Open this folder: {dir}\n\
         \x20 3. Move the current travelsnapmap.sqlite* files somewhere safe.\n\
         \x20 4. Copy travelsnapmap.sqlite from this backup there, and copy the folders inside files/ next to it.\n\
         \x20 5. Start TravelSnapMap.\n",
        dir = data_dir.display()
    )
}

// MARK: Export

/// Every place with its saved information and sources, as plain JSON.
pub fn places_json(db: &Database) -> Result<Value> {
    let mut out = Vec::new();
    for place in db.list_places(&PlaceFilter { sort: Some("name".into()), ..Default::default() })? {
        let facts: Vec<Value> = db
            .facts_for_place(&place.id)?
            .into_iter()
            .map(|f| json!({
                "type": f.fact_type, "text": f.text, "source": if f.reel_id.is_some() { "instagramReel" } else { "screenshot" },
                "sourceKind": f.source_kind, "sourceTimeSec": f.source_time_sec, "savedDate": f.valid_from,
                "creator": f.creator, "app": f.source_type,
            }))
            .collect();
        let trips: Vec<String> = db.trips_for_place(&place.id)?.into_iter().map(|t| t.name).collect();
        let reel_urls: Vec<String> = db.reels_for_place(&place.id)?.into_iter().map(|r| r.url).collect();
        out.push(json!({
            "id": place.id, "name": place.canonical_name, "alternativeNames": place.alternative_names,
            "latitude": place.latitude, "longitude": place.longitude, "address": place.address,
            "city": place.city, "region": place.region, "country": place.country, "countryCode": place.country_code,
            "category": place.category, "personalStatus": place.personal_status, "notes": place.notes,
            "verified": place.verification != Verification::NeedsReview, "sourceCount": place.source_count,
            "whySaved": place.summary_text, "facts": facts, "trips": trips, "reels": reel_urls,
            "createdAt": place.created_at, "updatedAt": place.updated_at,
        }));
    }
    Ok(json!({
        "exportedAt": chrono::Utc::now().to_rfc3339(),
        "app": format!("TravelSnapMap {}", env!("CARGO_PKG_VERSION")),
        "placeCount": out.len(),
        "places": out,
    }))
}

/// Verified places as a GeoJSON FeatureCollection (coordinates are [longitude, latitude]).
pub fn places_geojson(db: &Database) -> Result<Value> {
    let features: Vec<Value> = db
        .list_places(&PlaceFilter { verified_only: true, sort: Some("name".into()), ..Default::default() })?
        .into_iter()
        .map(|p| json!({
            "type": "Feature",
            "id": p.id,
            "geometry": { "type": "Point", "coordinates": [p.longitude, p.latitude] },
            "properties": {
                "name": p.canonical_name, "alternativeNames": p.alternative_names, "city": p.city, "region": p.region,
                "country": p.country, "countryCode": p.country_code, "address": p.address, "category": p.category,
                "personalStatus": p.personal_status, "notes": p.notes, "sourceCount": p.source_count,
                "userVerified": p.is_user_verified, "createdAt": p.created_at,
            },
        }))
        .collect();
    Ok(json!({ "type": "FeatureCollection", "features": features }))
}

pub fn write_json(value: &Value, dest: &Path) -> Result<PathBuf> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, serde_json::to_string_pretty(value)?)?;
    Ok(dest.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{DataOrigin, PlaceCandidate, PlaceCategory};

    fn candidate(name: &str, lat: f64, lon: f64) -> PlaceCandidate {
        PlaceCandidate { name: name.into(), latitude: lat, longitude: lon, city: Some("Kyoto".into()),
                         country: Some("Japan".into()), country_code: Some("JP".into()), ..Default::default() }
    }

    #[test]
    fn backup_contains_library_and_files_but_never_the_api_key() {
        let dir = std::env::temp_dir().join(format!("tsm-backup-{}", uuid::Uuid::new_v4()));
        let data = dir.join("data");
        std::fs::create_dir_all(data.join("crops")).unwrap();
        std::fs::create_dir_all(data.join("reels/r1")).unwrap();
        std::fs::write(data.join("crops/photo.jpg"), b"jpg").unwrap();
        std::fs::write(data.join("reels/r1/audio.m4a"), b"m4a").unwrap();
        std::fs::write(data.join("reels/r1/video.mp4"), b"big video").unwrap();
        let db = Database::open(&data.join("travelsnapmap.sqlite")).unwrap();
        db.insert_place(&candidate("Fushimi Inari", 34.967, 135.772), PlaceCategory::Temple, Verification::Verified, DataOrigin::MapKit, &[]).unwrap();
        db.set_setting("deepseekApiKey", Some("sk-THIS-MUST-NOT-LEAK")).unwrap();

        let zip = dir.join("out/backup.zip");
        let summary = create_backup(&db, &data, &AppConfig::default(), &zip).unwrap();
        assert_eq!((summary.places, summary.files), (1, 2), "crop + audio, not the video");

        let bytes = std::fs::read(&zip).unwrap();
        assert!(!bytes.windows(8).any(|w| w == b"sk-THIS-"), "API key must never be in a backup");
        let unpacked = dir.join("unpacked");
        assert!(std::process::Command::new("ditto").args(["-x", "-k"]).arg(&zip).arg(&unpacked).status().unwrap().success());
        let root = std::fs::read_dir(&unpacked).unwrap().next().unwrap().unwrap().path();
        assert!(root.join("config.json").exists() && root.join("README.txt").exists());
        assert!(root.join("files/crops/photo.jpg").exists() && root.join("files/reels/r1/audio.m4a").exists());
        assert!(!root.join("files/reels/r1/video.mp4").exists());
        let restored = Database::open(&root.join("travelsnapmap.sqlite")).unwrap();
        assert_eq!(restored.list_places(&PlaceFilter::default()).unwrap().len(), 1);
        assert!(restored.setting("deepseekApiKey").unwrap().is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn geojson_has_only_verified_places_with_lon_lat_order() {
        let db = Database::open_in_memory().unwrap();
        db.insert_place(&candidate("Kiyomizu-dera", 34.9949, 135.785), PlaceCategory::Temple, Verification::Verified, DataOrigin::MapKit, &[]).unwrap();
        db.insert_place(&candidate("Maybe Place", 35.0, 135.7), PlaceCategory::Other, Verification::NeedsReview, DataOrigin::MapKit, &[]).unwrap();
        let g = places_geojson(&db).unwrap();
        let features = g["features"].as_array().unwrap();
        assert_eq!(features.len(), 1);
        assert_eq!(features[0]["geometry"]["coordinates"], json!([135.785, 34.9949]));
        assert_eq!(features[0]["properties"]["name"], "Kiyomizu-dera");
        assert_eq!(features[0]["properties"]["personalStatus"], "wantToVisit");
        assert_eq!(features[0]["properties"]["sourceCount"], 0);
        let all = places_json(&db).unwrap();
        assert_eq!(all["placeCount"], 2);
    }
}
