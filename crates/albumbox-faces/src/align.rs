//! Face alignment (5-point similarity transform to the ArcFace template) and UI crops.

use image::{imageops::FilterType, Rgb, RgbImage};

/// ArcFace reference landmarks for a 112×112 face.
const TEMPLATE: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

/// Least-squares similarity (rotation + uniform scale + translation, no
/// reflection) mapping `src` onto `dst`: dst ≈ [a -b; b a]·src + t.
fn similarity(src: &[[f32; 2]; 5], dst: &[[f32; 2]; 5]) -> (f32, f32, f32, f32) {
    let n = src.len() as f32;
    let ms = src.iter().fold([0.0, 0.0], |m, p| [m[0] + p[0] / n, m[1] + p[1] / n]);
    let md = dst.iter().fold([0.0, 0.0], |m, p| [m[0] + p[0] / n, m[1] + p[1] / n]);
    let (mut num_a, mut num_b, mut den) = (0.0, 0.0, 0.0);
    for (s, d) in src.iter().zip(dst) {
        let (sx, sy) = (s[0] - ms[0], s[1] - ms[1]);
        let (dx, dy) = (d[0] - md[0], d[1] - md[1]);
        num_a += sx * dx + sy * dy;
        num_b += sx * dy - sy * dx;
        den += sx * sx + sy * sy;
    }
    let (a, b) = if den > 0.0 { (num_a / den, num_b / den) } else { (1.0, 0.0) };
    let tx = md[0] - (a * ms[0] - b * ms[1]);
    let ty = md[1] - (b * ms[0] + a * ms[1]);
    (a, b, tx, ty)
}

fn sample(img: &RgbImage, x: f32, y: f32) -> Rgb<u8> {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let (x0, y0) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let px = |xx: i64, yy: i64| -> [f32; 3] {
        if xx < 0 || yy < 0 || xx >= w || yy >= h {
            [0.0; 3]
        } else {
            let p = img.get_pixel(xx as u32, yy as u32);
            [p[0] as f32, p[1] as f32, p[2] as f32]
        }
    };
    let (p00, p10, p01, p11) = (px(x0, y0), px(x0 + 1, y0), px(x0, y0 + 1), px(x0 + 1, y0 + 1));
    let mut out = [0u8; 3];
    for c in 0..3 {
        let top = p00[c] * (1.0 - fx) + p10[c] * fx;
        let bottom = p01[c] * (1.0 - fx) + p11[c] * fx;
        out[c] = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
    }
    Rgb(out)
}

/// 112×112 aligned face for the recognizer.
pub fn align_arcface(img: &RgbImage, kps: &[[f32; 2]; 5]) -> RgbImage {
    let (a, b, tx, ty) = similarity(kps, &TEMPLATE);
    // Inverse of [a -b; b a]: (1/(a²+b²))·[a b; -b a]
    let det = (a * a + b * b).max(1e-12);
    RgbImage::from_fn(112, 112, |u, v| {
        let (dx, dy) = (u as f32 - tx, v as f32 - ty);
        let sx = (a * dx + b * dy) / det;
        let sy = (-b * dx + a * dy) / det;
        sample(img, sx, sy)
    })
}

/// Square crop around a box (x1, y1, x2, y2), padded 40%, resized to `size`.
pub fn square_crop(img: &RgbImage, bbox: [f32; 4], size: u32) -> RgbImage {
    let (cx, cy) = ((bbox[0] + bbox[2]) / 2.0, (bbox[1] + bbox[3]) / 2.0);
    let side = ((bbox[2] - bbox[0]).max(bbox[3] - bbox[1]) * 1.4).max(8.0);
    let max_side = img.width().min(img.height()) as f32;
    let side = side.min(max_side);
    let x = (cx - side / 2.0).clamp(0.0, img.width() as f32 - side) as u32;
    let y = (cy - side / 2.0).clamp(0.0, img.height() as f32 - side) as u32;
    let s = side as u32;
    let view = image::imageops::crop_imm(img, x, y, s.max(1), s.max(1)).to_image();
    image::imageops::resize(&view, size, size, FilterType::Triangle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn similarity_recovers_known_transform() {
        // dst = 2·R(90°)·src + (10, 20)
        let src = [[1.0, 0.0], [0.0, 1.0], [2.0, 2.0], [-1.0, 3.0], [4.0, -2.0]];
        let mut dst = [[0f32; 2]; 5];
        for (d, s) in dst.iter_mut().zip(&src) {
            *d = [-2.0 * s[1] + 10.0, 2.0 * s[0] + 20.0];
        }
        let (a, b, tx, ty) = similarity(&src, &dst);
        assert!((a - 0.0).abs() < 1e-4 && (b - 2.0).abs() < 1e-4);
        assert!((tx - 10.0).abs() < 1e-3 && (ty - 20.0).abs() < 1e-3);
    }

    #[test]
    fn template_maps_to_itself() {
        let img = RgbImage::from_fn(112, 112, |x, y| Rgb([x as u8, y as u8, 0]));
        let out = align_arcface(&img, &TEMPLATE);
        assert_eq!(out.get_pixel(50, 60), img.get_pixel(50, 60));
    }
}
