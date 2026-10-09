//! Folder source: any folder of screenshot images (e.g. exported from a phone), not only Apple Photos.
//! Files are identified as `file:///absolute/path`, so "only new screenshots" works the same way as
//! PhotoKit asset IDs.

use std::path::{Path, PathBuf};

use crate::services::native::AssetInfo;

pub const FILE_PREFIX: &str = "file://";
const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "heic", "heif", "webp", "tiff", "tif"];
const MAX_DEPTH: usize = 6;

#[cfg(test)]
pub fn is_file_source(asset_id: &str) -> bool {
    asset_id.starts_with(FILE_PREFIX)
}

pub fn file_path(asset_id: &str) -> Option<PathBuf> {
    asset_id.strip_prefix(FILE_PREFIX).map(PathBuf::from)
}

/// All image files in a folder (recursively), newest first.
pub fn scan_folder(root: &Path) -> std::io::Result<Vec<AssetInfo>> {
    let mut out = Vec::new();
    walk(root, 0, &mut out)?;
    out.sort_by(|a, b| b.creation_date.cmp(&a.creation_date));
    Ok(out)
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<AssetInfo>) -> std::io::Result<()> {
    if depth > MAX_DEPTH {
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        if meta.is_dir() {
            let _ = walk(&path, depth + 1, out);
        } else if path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e.to_lowercase().as_str())) {
            let time = meta.created().or_else(|_| meta.modified()).ok();
            let absolute = path.canonicalize().unwrap_or(path);
            out.push(AssetInfo {
                id: format!("{FILE_PREFIX}{}", absolute.to_string_lossy()),
                creation_date: time.map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()),
                width: 0,
                height: 0,
            });
        }
    }
    Ok(())
}

/// Result of copying dropped or picked image files into the app's own import folder.
#[derive(Debug, Default, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedFiles {
    /// New images copied in (they'll be processed).
    pub added: usize,
    /// Same image content was imported before (skipped, so nothing is processed twice).
    pub already_imported: usize,
    /// Not an image TravelSnapMap can read (or unreadable).
    pub skipped: usize,
}

pub fn is_image(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
}

/// Copies image files into `dest` under a name derived from their content, so the same picture dropped twice
/// (even renamed) is recognised and never processed again. Other files are skipped.
pub fn import_files(paths: &[PathBuf], dest: &Path) -> std::io::Result<ImportedFiles> {
    use sha2::{Digest, Sha256};
    std::fs::create_dir_all(dest)?;
    let mut result = ImportedFiles::default();
    for path in paths {
        let Ok(meta) = std::fs::metadata(path) else { result.skipped += 1; continue };
        if !meta.is_file() || !is_image(path) {
            result.skipped += 1;
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else { result.skipped += 1; continue };
        let hash = hex::encode(&Sha256::digest(&bytes)[..12]);
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png").to_lowercase();
        let target = dest.join(format!("{hash}.{ext}"));
        if target.exists() {
            result.already_imported += 1;
            continue;
        }
        // Write then rename, so a half-copied file is never picked up by a scan.
        let partial = dest.join(format!(".{hash}.partial"));
        std::fs::write(&partial, &bytes)?;
        std::fs::rename(&partial, &target)?;
        // Keep the original date so the library sorts it correctly.
        if let Ok(modified) = meta.modified() {
            let _ = std::fs::File::options().write(true).open(&target).and_then(|f| f.set_modified(modified));
        }
        result.added += 1;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn importing_files_copies_images_once_and_skips_the_rest() {
        let dir = std::env::temp_dir().join(format!("tsm-import-{}", uuid::Uuid::new_v4()));
        let inbox = dir.join("inbox");
        std::fs::create_dir_all(&inbox).unwrap();
        std::fs::write(inbox.join("IMG_1.PNG"), b"one").unwrap();
        std::fs::write(inbox.join("copy of IMG_1.png"), b"one").unwrap(); // same picture, renamed
        std::fs::write(inbox.join("IMG_2.jpg"), b"two").unwrap();
        std::fs::write(inbox.join("notes.txt"), b"hello").unwrap();
        let paths: Vec<PathBuf> = ["IMG_1.PNG", "copy of IMG_1.png", "IMG_2.jpg", "notes.txt", "missing.png"]
            .iter().map(|f| inbox.join(f)).collect();

        let dest = dir.join("imports");
        let first = import_files(&paths, &dest).unwrap();
        assert_eq!(first, ImportedFiles { added: 2, already_imported: 1, skipped: 2 });
        assert_eq!(scan_folder(&dest).unwrap().len(), 2, "no partial files are left behind");

        let again = import_files(&paths, &dest).unwrap();
        assert_eq!(again, ImportedFiles { added: 0, already_imported: 3, skipped: 2 }, "re-importing adds nothing");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn scans_images_recursively_and_ignores_others() {
        let dir = std::env::temp_dir().join(format!("tsm-folder-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("trip/sub")).unwrap();
        for f in ["a.PNG", "trip/b.jpg", "trip/sub/c.heic", "notes.txt", ".hidden.png"] {
            std::fs::write(dir.join(f), b"x").unwrap();
        }
        let assets = scan_folder(&dir).unwrap();
        assert_eq!(assets.len(), 3);
        assert!(assets.iter().all(|a| is_file_source(&a.id) && a.creation_date.is_some()));
        assert!(file_path(&assets[0].id).unwrap().exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
