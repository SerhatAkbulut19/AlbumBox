//! Per-file analysis: content hash and metadata. Blocking; run on a blocking thread.

use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use albumbox_core::media::{MediaKind, ScannedMedia};
use anyhow::Context;
use serde::Deserialize;

use crate::{dates, tools::{self, Tools}};

/// blake3 of the whole file, read in 1 MB blocks (sequential reads suit USB disks).
pub fn hash_file(path: &Path) -> anyhow::Result<String> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

pub fn analyze(path: &Path, kind: MediaKind, mtime: i64, tools: &Tools) -> anyhow::Result<ScannedMedia> {
    let mut m = ScannedMedia { hash: hash_file(path)?, kind: Some(kind), ..Default::default() };
    match kind {
        MediaKind::Photo => photo_metadata(path, &mut m),
        MediaKind::Video => {
            if let Err(e) = video_metadata(path, tools, &mut m) {
                tracing::debug!(file = %path.display(), error = %e, "ffprobe failed");
            }
        }
    }
    if m.taken_at.is_none() {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(t) = dates::from_filename(&name) {
            m.taken_at = Some(t);
            m.taken_at_source = Some("filename");
        } else if let Some(t) = dates::from_mtime(mtime) {
            m.taken_at = Some(t);
            m.taken_at_source = Some("mtime");
        }
    }
    Ok(m)
}

// ---------------------------------------------------------------------------
// Photos: EXIF + header dimensions
// ---------------------------------------------------------------------------

fn photo_metadata(path: &Path, m: &mut ScannedMedia) {
    let mut orientation = 1u32;
    if let Ok(file) = File::open(path) {
        if let Ok(exif) = exif::Reader::new().read_from_container(&mut BufReader::new(file)) {
            use exif::{In, Tag};
            let text = |tag| exif.get_field(tag, In::PRIMARY).map(|f| f.display_value().to_string());
            if let Some(t) = text(Tag::DateTimeOriginal).or_else(|| text(Tag::DateTimeDigitized)).and_then(|v| dates::from_exif(&v)) {
                m.taken_at = Some(t);
                m.taken_at_source = Some("exif");
            }
            let make = text(Tag::Make).map(|s| s.trim_matches('"').trim().to_string()).unwrap_or_default();
            let model = text(Tag::Model).map(|s| s.trim_matches('"').trim().to_string()).unwrap_or_default();
            let camera = if model.to_lowercase().starts_with(&make.to_lowercase()) || make.is_empty() {
                model
            } else {
                format!("{make} {model}")
            };
            m.camera = Some(camera).filter(|c| !c.is_empty());
            if let Some(o) = exif.get_field(Tag::Orientation, In::PRIMARY).and_then(|f| f.value.get_uint(0)) {
                orientation = o;
            }
            m.gps_lat = gps_coord(&exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, b'S');
            m.gps_lon = gps_coord(&exif, Tag::GPSLongitude, Tag::GPSLongitudeRef, b'W');
        }
    }
    if let Ok(size) = imagesize::size(path) {
        let (w, h) = (size.width as i64, size.height as i64);
        // EXIF orientations 5-8 rotate by 90°: displayed width/height swap.
        (m.width, m.height) = if (5..=8).contains(&orientation) { (Some(h), Some(w)) } else { (Some(w), Some(h)) };
    }
}

fn gps_coord(exif: &exif::Exif, tag: exif::Tag, ref_tag: exif::Tag, negative: u8) -> Option<f64> {
    let field = exif.get_field(tag, exif::In::PRIMARY)?;
    let exif::Value::Rational(ref parts) = field.value else { return None };
    if parts.len() < 3 || parts.iter().any(|r| r.denom == 0) {
        return None;
    }
    let value = parts[0].to_f64() + parts[1].to_f64() / 60.0 + parts[2].to_f64() / 3600.0;
    let sign = match exif.get_field(ref_tag, exif::In::PRIMARY).map(|f| &f.value) {
        Some(exif::Value::Ascii(v)) if v.first().and_then(|s| s.first()) == Some(&negative) => -1.0,
        _ => 1.0,
    };
    (value != 0.0).then_some(sign * value)
}

// ---------------------------------------------------------------------------
// Videos: ffprobe
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct Probe {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    #[serde(default)]
    tags: std::collections::HashMap<String, String>,
    #[serde(default)]
    side_data_list: Vec<serde_json::Value>,
}

#[derive(Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
    #[serde(default)]
    tags: std::collections::HashMap<String, String>,
}

fn video_metadata(path: &Path, tools: &Tools, m: &mut ScannedMedia) -> anyhow::Result<()> {
    let mut cmd = tools::command(&tools.ffprobe);
    cmd.args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]).arg(path);
    let out = tools::run(cmd)?;
    let probe: Probe = serde_json::from_slice(&out.stdout)?;

    if let Some(v) = probe.streams.iter().find(|s| s.codec_type.as_deref() == Some("video")) {
        let rotation = v
            .side_data_list
            .iter()
            .find_map(|d| d.get("rotation").and_then(|r| r.as_i64()))
            .or_else(|| v.tags.get("rotate").and_then(|r| r.parse().ok()))
            .unwrap_or(0);
        (m.width, m.height) = if rotation.abs() % 180 == 90 { (v.height, v.width) } else { (v.width, v.height) };
    }
    if let Some(f) = &probe.format {
        m.duration_ms = f.duration.as_deref().and_then(|d| d.parse::<f64>().ok()).map(|d| (d * 1000.0) as i64);
        let tag = |k: &str| f.tags.iter().find(|(key, _)| key.eq_ignore_ascii_case(k)).map(|(_, v)| v.as_str());
        if let Some(t) = tag("com.apple.quicktime.creationdate")
            .and_then(|v| time::OffsetDateTime::parse(v, &time::format_description::well_known::Iso8601::DEFAULT).ok())
            .map(|dt| dt.format(&time::format_description::well_known::Rfc3339).unwrap_or_default())
            .and_then(|v| dates::from_video_utc(&v))
            .or_else(|| tag("creation_time").and_then(dates::from_video_utc))
        {
            m.taken_at = Some(t);
            m.taken_at_source = Some("video_meta");
        }
        if let Some((lat, lon)) = tag("com.apple.quicktime.location.ISO6709").or_else(|| tag("location")).and_then(parse_iso6709) {
            m.gps_lat = Some(lat);
            m.gps_lon = Some(lon);
        }
        m.camera = tag("com.apple.quicktime.model").or_else(|| tag("model")).map(str::to_string);
    }
    Ok(())
}

/// "+41.0082+028.9784+035.000/" → (41.0082, 28.9784)
fn parse_iso6709(s: &str) -> Option<(f64, f64)> {
    let s = s.trim().trim_end_matches('/');
    let mut parts = Vec::new();
    let mut start = 0;
    for (i, c) in s.char_indices().skip(1) {
        if c == '+' || c == '-' {
            parts.push(&s[start..i]);
            start = i;
        }
    }
    parts.push(&s[start..]);
    let lat = parts.first()?.parse().ok()?;
    let lon = parts.get(1)?.parse().ok()?;
    Some((lat, lon))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso6709() {
        assert_eq!(parse_iso6709("+41.0082+028.9784+035.000/"), Some((41.0082, 28.9784)));
        assert_eq!(parse_iso6709("-33.8688+151.2093/"), Some((-33.8688, 151.2093)));
    }

    #[test]
    fn hash_is_content_based() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.jpg");
        let b = dir.path().join("b.jpg");
        std::fs::write(&a, b"ayni icerik").unwrap();
        std::fs::write(&b, b"ayni icerik").unwrap();
        assert_eq!(hash_file(&a).unwrap(), hash_file(&b).unwrap());
    }
}
