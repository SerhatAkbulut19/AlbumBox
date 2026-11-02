//! `.albumbox` marker file at a library root. It is the only file AlbumBox
//! ever writes to a library disk.

use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

pub const MARKER_FILE: &str = ".albumbox";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Marker {
    pub library_id: String,
    pub name: String,
    pub created_at: String,
}

pub fn read(root: &Path) -> Option<Marker> {
    let text = std::fs::read_to_string(root.join(MARKER_FILE)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes a new marker (hidden on Windows) and returns it.
pub fn create(root: &Path, name: &str) -> anyhow::Result<Marker> {
    let marker = Marker {
        library_id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        created_at: time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_default(),
    };
    let path = root.join(MARKER_FILE);
    let json = serde_json::to_string_pretty(&marker)?;
    write_hidden(&path, json.as_bytes()).with_context(|| format!("write {}", path.display()))?;
    Ok(marker)
}

#[cfg(windows)]
fn write_hidden(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::{io::Write, os::windows::fs::OpenOptionsExt};
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .attributes(FILE_ATTRIBUTE_HIDDEN)
        .open(path)?;
    f.write_all(bytes)
}

#[cfg(not(windows))]
fn write_hidden(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::OpenOptions::new().write(true).create_new(true).open(path)?.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_and_read() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read(dir.path()).is_none());
        let m = create(dir.path(), " Aile Diski ").unwrap();
        assert_eq!(m.name, "Aile Diski");
        assert_eq!(read(dir.path()), Some(m));
        assert!(create(dir.path(), "x").is_err(), "never overwrite an existing marker");
    }
}
