use std::path::Path;

use albumbox_core::media::MediaKind;

const PHOTO: &[&str] = &["jpg", "jpeg", "jpe", "png", "webp", "gif", "bmp", "tif", "tiff", "heic", "heif"];
const VIDEO: &[&str] =
    &["mp4", "m4v", "mov", "3gp", "3g2", "avi", "mkv", "mts", "m2ts", "wmv", "mpg", "mpeg", "webm", "flv", "vob"];

/// Media kind by file extension; `None` for files AlbumBox ignores.
pub fn kind_of(path: &Path) -> Option<MediaKind> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if PHOTO.contains(&ext.as_str()) {
        Some(MediaKind::Photo)
    } else if VIDEO.contains(&ext.as_str()) {
        Some(MediaKind::Video)
    } else {
        None
    }
}

/// Folders never scanned: Windows system folders and hidden dot-folders.
pub fn is_skipped_dir(name: &str) -> bool {
    name.starts_with('.')
        || name.starts_with('$')
        || name.eq_ignore_ascii_case("System Volume Information")
        || name.eq_ignore_ascii_case("@eaDir") // Synology thumbnails
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies() {
        assert_eq!(kind_of(Path::new("a/IMG_1.JPG")), Some(MediaKind::Photo));
        assert_eq!(kind_of(Path::new("VID.MOV")), Some(MediaKind::Video));
        assert_eq!(kind_of(Path::new("Thumbs.db")), None);
        assert!(is_skipped_dir("$RECYCLE.BIN"));
        assert!(!is_skipped_dir("Köy 2022"));
    }
}
