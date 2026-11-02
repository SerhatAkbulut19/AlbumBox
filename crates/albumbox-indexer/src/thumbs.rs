//! Thumbnails: JPEG, long edge 256 (grid) and 1024 (viewer / offline preview).
//! Stored at `<cache>/thumbs/<hash[0..2]>/<hash>_<size>.jpg`.

use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
};

use albumbox_core::media::MediaKind;
use anyhow::Context;
use fast_image_resize::{images::Image, IntoImageView, ResizeOptions, Resizer};
use image::{codecs::jpeg::JpegEncoder, DynamicImage, ImageDecoder, ImageReader, RgbImage};

use crate::tools::{self, Tools};

pub const SIZES: [u32; 2] = [256, 1024];

pub fn thumb_path(cache_dir: &Path, hash: &str, size: u32) -> PathBuf {
    let shard = hash.get(..2).unwrap_or("00");
    cache_dir.join("thumbs").join(shard).join(format!("{hash}_{size}.jpg"))
}

/// Builds both sizes; returns the oriented source dimensions.
pub fn generate(
    src: &Path,
    kind: MediaKind,
    duration_ms: Option<i64>,
    cache_dir: &Path,
    hash: &str,
    tools: &Tools,
) -> anyhow::Result<(u32, u32)> {
    let out_dir = thumb_path(cache_dir, hash, 0).parent().map(Path::to_path_buf).context("cache path")?;
    std::fs::create_dir_all(&out_dir)?;

    let (source, dims) = match kind {
        MediaKind::Photo => match decode_photo(src) {
            Ok(img) => {
                let dims = (img.width(), img.height());
                (img, dims)
            }
            // HEIC and exotic formats: let ffmpeg decode them.
            Err(e) => {
                tracing::debug!(file = %src.display(), error = %e, "image decode failed, trying ffmpeg");
                ffmpeg_frame(src, None, tools)?
            }
        },
        MediaKind::Video => {
            // A frame a little into the video avoids black fade-ins.
            let at = duration_ms.map(|d| (d as f64 / 1000.0 * 0.1).min(3.0)).filter(|s| *s > 0.0);
            ffmpeg_frame(src, at, tools)?
        }
    };

    let rgb = source.to_rgb8();
    let mut resizer = Resizer::new();
    for size in SIZES {
        let thumb = resize_to_fit(&mut resizer, &rgb, size)?;
        write_jpeg(&thumb, &thumb_path(cache_dir, hash, size), if size >= 1024 { 82 } else { 78 })?;
    }
    Ok(dims)
}

fn decode_photo(src: &Path) -> anyhow::Result<DynamicImage> {
    let mut decoder = ImageReader::open(src)?.with_guessed_format()?.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut img = DynamicImage::from_decoder(decoder)?;
    img.apply_orientation(orientation);
    Ok(img)
}

/// One frame as a ≤1024 px image via ffmpeg (auto-rotates videos).
fn ffmpeg_frame(src: &Path, at_secs: Option<f64>, tools: &Tools) -> anyhow::Result<(DynamicImage, (u32, u32))> {
    let tmp = std::env::temp_dir().join(format!("albumbox-{}.png", uuid::Uuid::new_v4()));
    let mut cmd = tools::command(&tools.ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
    if let Some(at) = at_secs {
        cmd.args(["-ss", &format!("{at:.2}")]);
    }
    cmd.arg("-i").arg(src).args(["-frames:v", "1", "-vf", "scale=w=1024:h=1024:force_original_aspect_ratio=decrease"]);
    cmd.arg(&tmp);
    let result = tools::run(cmd).and_then(|_| Ok(image::open(&tmp)?));
    let _ = std::fs::remove_file(&tmp);
    let img = result?;
    let dims = (img.width(), img.height());
    Ok((img, dims))
}

fn resize_to_fit(resizer: &mut Resizer, src: &RgbImage, max: u32) -> anyhow::Result<RgbImage> {
    let (w, h) = src.dimensions();
    if w <= max && h <= max {
        return Ok(src.clone());
    }
    let scale = max as f64 / w.max(h) as f64;
    let (tw, th) = (((w as f64 * scale).round() as u32).max(1), ((h as f64 * scale).round() as u32).max(1));
    let mut dst = Image::new(tw, th, src.pixel_type().context("pixel type")?);
    resizer.resize(src, &mut dst, &ResizeOptions::new())?;
    RgbImage::from_raw(tw, th, dst.into_vec()).context("resize buffer")
}

fn write_jpeg(img: &RgbImage, path: &Path, quality: u8) -> anyhow::Result<()> {
    // Write to a temp name first so readers never see half-written files.
    let tmp = path.with_extension("tmp");
    {
        let mut out = BufWriter::new(File::create(&tmp)?);
        JpegEncoder::new_with_quality(&mut out, quality).encode_image(img)?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photo_thumbnails_keep_aspect_ratio() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("foto.png");
        RgbImage::from_fn(2000, 1000, |x, y| image::Rgb([(x % 255) as u8, (y % 255) as u8, 120])).save(&src).unwrap();

        let dims = generate(&src, MediaKind::Photo, None, dir.path(), "abcdef", &Tools::default()).unwrap();
        assert_eq!(dims, (2000, 1000));
        let small = image::open(thumb_path(dir.path(), "abcdef", 256)).unwrap();
        assert_eq!((small.width(), small.height()), (256, 128));
        let big = image::open(thumb_path(dir.path(), "abcdef", 1024)).unwrap();
        assert_eq!((big.width(), big.height()), (1024, 512));
    }
}
