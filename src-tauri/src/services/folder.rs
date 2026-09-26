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

#[cfg(test)]
mod tests {
    use super::*;

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
