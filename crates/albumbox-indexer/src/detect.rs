//! Finding library roots after the drive letter changed.

use std::path::{Component, Path, PathBuf};

use crate::marker;

/// Mounted drive roots (`C:\` … `Z:\`). A and B are skipped (legacy floppy).
#[cfg(windows)]
pub fn mounted_drives() -> Vec<PathBuf> {
    (b'C'..=b'Z').map(|l| PathBuf::from(format!("{}:\\", l as char))).filter(|p| p.exists()).collect()
}

#[cfg(not(windows))]
pub fn mounted_drives() -> Vec<PathBuf> {
    vec![PathBuf::from("/")]
}

/// Splits `F:\Aile\Fotoğraflar` into (`F:\`, `Aile\Fotoğraflar`).
pub fn split_drive(path: &Path) -> (PathBuf, String) {
    let mut drive = PathBuf::new();
    let mut rest = Vec::new();
    for c in path.components() {
        match c {
            Component::Prefix(_) | Component::RootDir => drive.push(c.as_os_str()),
            Component::Normal(s) => rest.push(s.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                rest.pop();
            }
        }
    }
    (drive, rest.join("\\"))
}

fn join_subpath(drive: &Path, subpath: &str) -> PathBuf {
    if subpath.is_empty() {
        drive.to_path_buf()
    } else {
        subpath.split('\\').fold(drive.to_path_buf(), |p, s| p.join(s))
    }
}

/// Where a library is mounted now: first its last known root, then the same
/// sub-path on every candidate drive. The marker id must match.
pub fn locate(library_id: &str, root_subpath: &str, last_root: Option<&str>, drives: &[PathBuf]) -> Option<PathBuf> {
    let matches = |root: &Path| marker::read(root).is_some_and(|m| m.library_id == library_id);
    if let Some(last) = last_root.map(PathBuf::from) {
        if matches(&last) {
            return Some(last);
        }
    }
    drives.iter().map(|d| join_subpath(d, root_subpath)).find(|root| matches(root))
}

/// Root path as stored in the database (no trailing separator except for drive roots).
pub fn root_string(root: &Path) -> String {
    root.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_library_on_another_drive() {
        // Two temp dirs stand in for drive letters E:\ and F:\.
        let e = tempfile::tempdir().unwrap();
        let f = tempfile::tempdir().unwrap();
        let root_on_f = f.path().join("Aile");
        std::fs::create_dir_all(&root_on_f).unwrap();
        let m = marker::create(&root_on_f, "Aile").unwrap();

        let drives = vec![e.path().to_path_buf(), f.path().to_path_buf()];
        // Last seen on E: (e.g. E:\Aile), now plugged in as F:.
        let last = e.path().join("Aile");
        let found = locate(&m.library_id, "Aile", Some(&last.to_string_lossy()), &drives);
        assert_eq!(found, Some(root_on_f.clone()));
        assert_eq!(locate("baska-id", "Aile", None, &drives), None);
    }

    #[cfg(windows)]
    #[test]
    fn splits_drive_and_subpath() {
        assert_eq!(split_drive(Path::new(r"F:\Aile\Fotoğraflar")), (PathBuf::from(r"F:\"), r"Aile\Fotoğraflar".into()));
        assert_eq!(split_drive(Path::new(r"F:\")), (PathBuf::from(r"F:\"), String::new()));
        assert_eq!(join_subpath(Path::new(r"G:\"), r"Aile\Foto"), PathBuf::from(r"G:\Aile\Foto"));
    }
}
