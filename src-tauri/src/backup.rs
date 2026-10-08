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

/// A trip as a Markdown itinerary: days in order, each place with where it is, your notes, the practical tips you
/// saved (with their date — they may be out of date) and an Apple Maps link. Nothing is invented.
pub fn trip_markdown(db: &Database, trip_id: &str) -> Result<String> {
    use crate::models::TravelFactType as F;
    use std::fmt::Write;
    let trip = db.trip(trip_id)?.ok_or_else(|| anyhow::anyhow!("trip not found"))?;
    let entries = db.trip_entries(trip_id)?;
    let mut md = format!("# {}\n\n", trip.name.trim());
    let dates = match (&trip.start_date, &trip.end_date) {
        (Some(a), Some(b)) => format!("{a} – {b} · "),
        (Some(a), None) => format!("From {a} · "),
        _ => String::new(),
    };
    writeln!(md, "{dates}{} place{}\n", entries.len(), if entries.len() == 1 { "" } else { "s" })?;
    if !trip.notes.trim().is_empty() {
        writeln!(md, "{}\n", trip.notes.trim())?;
    }
    let practical = [F::OpeningHours, F::Price, F::Ticket, F::Reservation, F::RecommendedTime, F::Warning];
    let mut current: Option<Option<i64>> = None;
    let mut n = 0;
    for e in &entries {
        if current != Some(e.day) {
            current = Some(e.day);
            n = 0;
            match e.day {
                Some(d) => writeln!(md, "## Day {d}\n")?,
                None => writeln!(md, "## Not scheduled yet\n")?,
            }
        }
        n += 1;
        let p = &e.place;
        let place_line = [p.city.as_deref(), p.country.as_deref()].into_iter().flatten().collect::<Vec<_>>().join(", ");
        writeln!(md, "{n}. **{}** — {}{}", p.canonical_name, category_label(p.category.as_str()),
                 if place_line.is_empty() { String::new() } else { format!(" · {place_line}") })?;
        if p.personal_status.as_str() == "visited" {
            writeln!(md, "   - Visited{}", p.visited_at.as_deref().map(|d| format!(" on {d}")).unwrap_or_default())?;
        }
        if !p.notes.trim().is_empty() {
            writeln!(md, "   - Your notes: {}", p.notes.trim().replace('\n', " "))?;
        }
        let facts = db.facts_for_place(&p.id)?;
        for f in facts.iter().filter(|f| practical.contains(&f.fact_type)).take(4) {
            let saved = f.valid_from.as_deref().map(|d| format!(" _(saved {})_", &d[..d.len().min(10)])).unwrap_or_default();
            writeln!(md, "   - {}{saved}", f.text.trim())?;
        }
        writeln!(md, "   - [Open in Apple Maps](https://maps.apple.com/?ll={:.6},{:.6}&q={})", p.latitude, p.longitude, url_encode(&p.canonical_name))?;
    }
    writeln!(md, "\n---\n_Exported from TravelSnapMap. Tips come from your saved screenshots and Reels and may be out of date — check before you go._")?;
    Ok(md)
}

fn category_label(category: &str) -> String {
    let mut words = String::new();
    for (i, c) in category.chars().enumerate() {
        if c.is_uppercase() && i > 0 { words.push(' '); }
        words.push(if i == 0 { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() });
    }
    words
}

fn url_encode(s: &str) -> String {
    s.bytes().map(|b| match b {
        b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
        _ => format!("%{b:02X}"),
    }).collect()
}

// MARK: Restore

/// Folder (inside the data directory) holding a checked backup that replaces the library on next launch.
pub const PENDING_RESTORE: &str = "restore-pending";

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RestorePreview {
    pub places: i64,
    pub screenshots: i64,
    pub reels: i64,
    pub trips: i64,
    pub schema_version: i64,
}

/// Checks that a file is a healthy TravelSnapMap library this version can open (read-only; nothing is changed).
pub fn inspect_backup_library(db_path: &Path) -> Result<RestorePreview> {
    use rusqlite::OpenFlags;
    let conn = rusqlite::Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .context("This backup doesn't contain a readable TravelSnapMap library")?;
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0)).context("not a TravelSnapMap library")?;
    anyhow::ensure!(version >= 1, "This file isn't a TravelSnapMap library.");
    anyhow::ensure!(version <= crate::db::SCHEMA_VERSION,
        "This backup was made by a newer version of TravelSnapMap (library v{version}). Update the app first.");
    let check: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    anyhow::ensure!(check == "ok", "This backup's library is damaged ({check}).");
    let count = |table: &str| -> Result<i64> {
        Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)).context("not a TravelSnapMap library")?)
    };
    let reels = if version >= 2 { count("reels")? } else { 0 };
    Ok(RestorePreview { places: count("places")?, screenshots: count("screenshots")?, reels, trips: count("trips")?, schema_version: version })
}

/// The folder with `travelsnapmap.sqlite` in an unpacked backup (the backup's own folder, or the root).
fn backup_root(unpacked: &Path) -> Result<PathBuf> {
    if unpacked.join("travelsnapmap.sqlite").is_file() {
        return Ok(unpacked.to_path_buf());
    }
    for entry in std::fs::read_dir(unpacked)? {
        let path = entry?.path();
        if path.join("travelsnapmap.sqlite").is_file() {
            return Ok(path);
        }
    }
    anyhow::bail!("This zip isn't a TravelSnapMap backup (no library inside).")
}

/// Unpacks and checks a backup, then stages it to replace the library the next time the app starts. The current
/// library isn't touched now; on restart it is moved to `backups/before-restore-…` (never deleted).
pub fn stage_restore(zip: &Path, data_dir: &Path) -> Result<RestorePreview> {
    let unpacked = data_dir.join(format!(".restore-{}", uuid::Uuid::new_v4()));
    let result = (|| -> Result<RestorePreview> {
        let status = std::process::Command::new("ditto").args(["-x", "-k"]).arg(zip).arg(&unpacked).status()
            .context("unpacking the backup")?;
        anyhow::ensure!(status.success(), "This file couldn't be unpacked. Is it a TravelSnapMap backup (.zip)?");
        stage_unpacked(&unpacked, data_dir)
    })();
    let _ = std::fs::remove_dir_all(&unpacked);
    result
}

/// Second half of `stage_restore`, for an already unpacked backup.
pub fn stage_unpacked(unpacked: &Path, data_dir: &Path) -> Result<RestorePreview> {
    let root = backup_root(unpacked)?;
    let preview = inspect_backup_library(&root.join("travelsnapmap.sqlite"))?;
    let pending = data_dir.join(PENDING_RESTORE);
    let _ = std::fs::remove_dir_all(&pending);
    std::fs::rename(&root, &pending).or_else(|_| copy_tree(&root, &pending).map(|_| ()))?;
    Ok(preview)
}

/// On launch, before the library is opened: swaps in a staged restore. The current library and its files are moved
/// to `backups/before-restore-<time>/`; if anything fails they are moved back. Returns where the old library went.
pub fn apply_pending_restore(data_dir: &Path) -> Result<Option<PathBuf>> {
    let pending = data_dir.join(PENDING_RESTORE);
    if !pending.join("travelsnapmap.sqlite").is_file() {
        return Ok(None);
    }
    let keep = data_dir.join("backups").join(format!("before-restore-{}", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    std::fs::create_dir_all(&keep)?;
    let mut items: Vec<String> = ["travelsnapmap.sqlite", "travelsnapmap.sqlite-wal", "travelsnapmap.sqlite-shm"].map(String::from).to_vec();
    items.extend(INCLUDED_DIRS.iter().map(|d| d.to_string()));
    let mut moved_out = Vec::new();
    let mut moved_in = Vec::new();
    let result = (|| -> Result<()> {
        for name in &items {
            if data_dir.join(name).exists() {
                std::fs::rename(data_dir.join(name), keep.join(name))?;
                moved_out.push(name.clone());
            }
        }
        std::fs::rename(pending.join("travelsnapmap.sqlite"), data_dir.join("travelsnapmap.sqlite"))?;
        moved_in.push("travelsnapmap.sqlite".to_string());
        for dir in INCLUDED_DIRS {
            let src = pending.join("files").join(dir);
            if src.is_dir() {
                std::fs::rename(&src, data_dir.join(dir))?;
                moved_in.push(dir.to_string());
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        // Put everything back the way it was.
        for name in &moved_in {
            let _ = std::fs::remove_dir_all(data_dir.join(name)).or_else(|_| std::fs::remove_file(data_dir.join(name)));
        }
        for name in &moved_out {
            let _ = std::fs::rename(keep.join(name), data_dir.join(name));
        }
        return Err(e.context("restoring the backup failed; your library was left as it was"));
    }
    let _ = std::fs::remove_dir_all(&pending);
    log::info!("restored a backup; the previous library is in {}", keep.display());
    Ok(Some(keep))
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
    #[cfg_attr(not(target_os = "macos"), ignore = "zips with macOS ditto")]
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

    /// Restore: a checked backup replaces the library on next launch; the old library is kept, not deleted.
    #[test]
    fn restore_swaps_in_the_backup_and_keeps_the_old_library() {
        let dir = std::env::temp_dir().join(format!("tsm-restore-{}", uuid::Uuid::new_v4()));
        let data = dir.join("data");
        // The library in use: one place, one crop.
        std::fs::create_dir_all(data.join("crops")).unwrap();
        std::fs::write(data.join("crops/current.jpg"), b"now").unwrap();
        let db = Database::open(&data.join("travelsnapmap.sqlite")).unwrap();
        db.insert_place(&candidate("Current place", 1.0, 1.0), PlaceCategory::Other, Verification::Verified, DataOrigin::MapKit, &[]).unwrap();
        drop(db);
        // An unpacked backup with two places and a trip.
        let unpacked = dir.join("unpacked/TravelSnapMap Backup 2026-01-01 1200");
        std::fs::create_dir_all(unpacked.join("files/crops")).unwrap();
        std::fs::write(unpacked.join("files/crops/old.jpg"), b"then").unwrap();
        let backup = Database::open(&unpacked.join("travelsnapmap.sqlite")).unwrap();
        backup.insert_place(&candidate("Fushimi Inari", 34.967, 135.772), PlaceCategory::Temple, Verification::Verified, DataOrigin::MapKit, &[]).unwrap();
        backup.insert_place(&candidate("Kiyomizu-dera", 34.994, 135.785), PlaceCategory::Temple, Verification::Verified, DataOrigin::MapKit, &[]).unwrap();
        backup.create_trip("Kyoto").unwrap();
        drop(backup);

        let preview = stage_unpacked(&dir.join("unpacked"), &data).unwrap();
        assert_eq!((preview.places, preview.trips, preview.schema_version), (2, 1, crate::db::SCHEMA_VERSION));
        assert_eq!(Database::open(&data.join("travelsnapmap.sqlite")).unwrap().list_places(&PlaceFilter::default()).unwrap().len(), 1,
                   "nothing changes until the app restarts");

        let kept = apply_pending_restore(&data).unwrap().expect("restore applied");
        let restored = Database::open(&data.join("travelsnapmap.sqlite")).unwrap();
        assert_eq!(restored.list_places(&PlaceFilter::default()).unwrap().len(), 2);
        assert!(data.join("crops/old.jpg").exists() && !data.join("crops/current.jpg").exists());
        assert!(kept.join("travelsnapmap.sqlite").exists() && kept.join("crops/current.jpg").exists(), "old library kept");
        assert!(!data.join(PENDING_RESTORE).exists());
        assert_eq!(apply_pending_restore(&data).unwrap(), None, "applied once");
        drop(restored);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn restore_refuses_files_that_are_not_a_healthy_library() {
        let dir = std::env::temp_dir().join(format!("tsm-badrestore-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::write(dir.join("a/travelsnapmap.sqlite"), b"definitely not sqlite").unwrap();
        assert!(stage_unpacked(&dir.join("a"), &dir.join("data")).is_err());
        std::fs::create_dir_all(dir.join("b")).unwrap();
        rusqlite::Connection::open(dir.join("b/travelsnapmap.sqlite")).unwrap().execute_batch("PRAGMA user_version = 99").unwrap();
        let err = stage_unpacked(&dir.join("b"), &dir.join("data")).unwrap_err().to_string();
        assert!(err.contains("newer version"), "{err}");
        std::fs::create_dir_all(dir.join("c")).unwrap();
        assert!(stage_unpacked(&dir.join("c"), &dir.join("data")).unwrap_err().to_string().contains("isn't a TravelSnapMap backup"));
        assert!(!dir.join("data").join(PENDING_RESTORE).exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn trip_export_lists_days_in_order_with_saved_tips_only() {
        let db = Database::open_in_memory().unwrap();
        let a = db.insert_place(&candidate("Fushimi Inari", 34.967, 135.772), PlaceCategory::Temple, Verification::Verified, DataOrigin::MapKit, &[]).unwrap();
        let b = db.insert_place(&candidate("Nishiki Market", 35.005, 135.764), PlaceCategory::Food, Verification::Verified, DataOrigin::MapKit, &[]).unwrap();
        let c = db.insert_place(&candidate("Kiyomizu-dera", 34.994, 135.785), PlaceCategory::Temple, Verification::Verified, DataOrigin::MapKit, &[]).unwrap();
        db.insert_fact(&a.id, crate::models::TravelFactType::RecommendedTime, "Go before 8 AM", None, 0.9,
            &crate::db::FactProvenance { screenshot_id: None, reel_id: None, kind: "user", time_sec: None, block_ids: vec![], valid_from: Some("2025-03-01T00:00:00Z") },
            DataOrigin::User).unwrap();
        let trip = db.create_trip("Kyoto weekend").unwrap();
        for p in [&a, &b, &c] { db.add_to_trip(&trip, &p.id).unwrap(); }
        let entries = db.trip_entries(&trip).unwrap();
        let id = |name: &str| entries.iter().find(|e| e.place.canonical_name == name).unwrap().id.clone();
        // Drag and drop: Nishiki first on day 1, then Fushimi on day 1; Kiyomizu unscheduled.
        db.reorder_trip(&trip, &[(id("Nishiki Market"), Some(1)), (id("Fushimi Inari"), Some(1)), (id("Kiyomizu-dera"), None)]).unwrap();

        let md = trip_markdown(&db, &trip).unwrap();
        let (nishiki, fushimi, later) = (md.find("Nishiki").unwrap(), md.find("Fushimi").unwrap(), md.find("## Not scheduled").unwrap());
        assert!(md.starts_with("# Kyoto weekend") && md.contains("## Day 1"));
        assert!(nishiki < fushimi && fushimi < later, "{md}");
        assert!(md.contains("Go before 8 AM _(saved 2025-03-01)_"));
        assert!(md.contains("https://maps.apple.com/?ll=34.967000,135.772000&q=Fushimi%20Inari"));
    }
}
