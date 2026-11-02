//! Admin helpers: adding a library and browsing server folders to pick one.

use std::path::{Path, PathBuf};

use albumbox_core::{libraries, CoreError, CoreResult, Db};
use serde::Serialize;

use crate::{detect, marker};

/// Turns a folder into a library: reuses an existing `.albumbox` marker
/// (e.g. after a database reset) or writes a new one.
pub async fn add_library(db: &Db, path: &str, name: &str) -> CoreResult<libraries::Library> {
    let path = PathBuf::from(path.trim());
    if !path.is_absolute() {
        return Err(CoreError::Invalid("path_not_absolute"));
    }
    let existing = libraries::list(db).await?;
    let new_root = normalize_root(&path);
    for lib in &existing {
        if let Some(root) = lib.last_seen_root.as_deref().filter(|_| lib.is_online) {
            let root = normalize_root(Path::new(root));
            if new_root == root || new_root.starts_with(&format!("{root}\\")) || root.starts_with(&format!("{new_root}\\")) {
                return Err(CoreError::Conflict("library_nested"));
            }
        }
    }

    let name = name.trim().to_string();
    let p = path.clone();
    let marker = tokio::task::spawn_blocking(move || -> CoreResult<marker::Marker> {
        if !p.is_dir() {
            return Err(CoreError::Invalid("path_not_found"));
        }
        if let Some(m) = marker::read(&p) {
            return Ok(m);
        }
        if name.is_empty() {
            return Err(CoreError::Invalid("name_required"));
        }
        marker::create(&p, &name).map_err(|e| {
            tracing::warn!(error = %format!("{e:#}"), "marker write failed");
            CoreError::Invalid("disk_read_only")
        })
    })
    .await
    .map_err(|e| CoreError::Other(e.into()))??;

    if existing.iter().any(|l| l.id == marker.library_id) {
        return Err(CoreError::Conflict("library_exists"));
    }
    let (_, subpath) = detect::split_drive(&path);
    libraries::insert(db, &marker.library_id, &marker.name, &subpath, &detect::root_string(&path)).await
}

fn normalize_root(p: &Path) -> String {
    p.to_string_lossy().trim_end_matches(['\\', '/']).to_lowercase()
}

#[derive(Debug, Serialize)]
pub struct FsEntry {
    pub name: String,
    pub path: String,
    /// Folder already holds an `.albumbox` marker.
    pub is_library: bool,
}

#[derive(Debug, Serialize)]
pub struct FsListing {
    /// `None` = drive list.
    pub path: Option<String>,
    pub parent: Option<String>,
    pub entries: Vec<FsEntry>,
}

/// Drives when `path` is empty, otherwise the sub-folders of `path`. Blocking.
pub fn browse(path: Option<&str>) -> CoreResult<FsListing> {
    let Some(path) = path.map(str::trim).filter(|p| !p.is_empty()) else {
        let entries = detect::mounted_drives()
            .into_iter()
            .map(|d| {
                let s = d.to_string_lossy().into_owned();
                FsEntry { is_library: marker::read(&d).is_some(), name: s.clone(), path: s }
            })
            .collect();
        return Ok(FsListing { path: None, parent: None, entries });
    };
    let dir = PathBuf::from(path);
    let read = std::fs::read_dir(&dir).map_err(|_| CoreError::Invalid("path_not_found"))?;
    let mut entries: Vec<FsEntry> = read
        .filter_map(Result::ok)
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if crate::kinds::is_skipped_dir(&name) {
                return None;
            }
            let p = e.path();
            Some(FsEntry { is_library: marker::read(&p).is_some(), path: p.to_string_lossy().into_owned(), name })
        })
        .collect();
    entries.sort_by_key(|e| e.name.to_lowercase());
    let parent = dir.parent().map(|p| p.to_string_lossy().into_owned());
    Ok(FsListing { path: Some(dir.to_string_lossy().into_owned()), parent: parent.or(Some(String::new())), entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn add_reuses_marker_and_rejects_nesting() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Aile");
        std::fs::create_dir_all(root.join("Köy")).unwrap();
        let db = Db::open_memory().await.unwrap();

        let lib = add_library(&db, &root.to_string_lossy(), "Aile Diski").await.unwrap();
        assert_eq!(marker::read(&root).unwrap().library_id, lib.id);
        assert!(matches!(
            add_library(&db, &root.join("Köy").to_string_lossy(), "Alt").await,
            Err(CoreError::Conflict("library_nested"))
        ));

        // Database lost: adding the same folder again keeps the original id.
        let db2 = Db::open_memory().await.unwrap();
        let again = add_library(&db2, &root.to_string_lossy(), "").await.unwrap();
        assert_eq!(again.id, lib.id);
        assert_eq!(again.name, "Aile Diski");

        let listing = browse(Some(&tmp.path().to_string_lossy())).unwrap();
        assert!(listing.entries.iter().any(|e| e.name == "Aile" && e.is_library));
    }
}
