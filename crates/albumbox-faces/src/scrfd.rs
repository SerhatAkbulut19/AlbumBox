//! SCRFD face detector: letterbox pre-processing and anchor decoding.

use anyhow::bail;
use image::{imageops::FilterType, RgbImage};
use ndarray::Array4;

pub const INPUT: u32 = 640;
const STRIDES: [usize; 3] = [8, 16, 32];
const ANCHORS: usize = 2;
const SCORE_THRESHOLD: f32 = 0.5;
const NMS_IOU: f32 = 0.4;

#[derive(Debug, Clone)]
pub struct Detection {
    /// x1, y1, x2, y2 in source image pixels.
    pub bbox: [f32; 4],
    pub score: f32,
    /// 5 landmarks (eyes, nose, mouth corners) in source image pixels.
    pub kps: [[f32; 2]; 5],
}

/// Resizes into a 640×640 canvas (top-left, aspect kept), normalized
/// `(p - 127.5) / 128`, NCHW RGB. Returns the tensor and the scale factor.
pub fn preprocess(img: &RgbImage) -> (Array4<f32>, f32) {
    let (w, h) = (img.width() as f32, img.height() as f32);
    let scale = (INPUT as f32 / w).min(INPUT as f32 / h);
    let (nw, nh) = (((w * scale).round() as u32).max(1), ((h * scale).round() as u32).max(1));
    let resized = image::imageops::resize(img, nw, nh, FilterType::Triangle);
    let mut input = Array4::<f32>::zeros((1, 3, INPUT as usize, INPUT as usize));
    for (x, y, p) in resized.enumerate_pixels() {
        for c in 0..3 {
            input[[0, c, y as usize, x as usize]] = (p[c] as f32 - 127.5) / 128.0;
        }
    }
    // The unused area stays 0.0 after normalization, as in InsightFace.
    (input, scale)
}

/// Decodes the 9 outputs (scores ×3, boxes ×3, landmarks ×3 for strides 8/16/32).
pub fn postprocess(outputs: &[Vec<f32>], scale: f32) -> anyhow::Result<Vec<Detection>> {
    if outputs.len() < 9 {
        bail!("SCRFD: 9 çıktı beklenirken {} geldi (landmark'sız model desteklenmiyor)", outputs.len());
    }
    let mut found = Vec::new();
    for (i, &stride) in STRIDES.iter().enumerate() {
        let (scores, boxes, kps) = (&outputs[i], &outputs[i + 3], &outputs[i + 6]);
        let side = INPUT as usize / stride;
        let count = side * side * ANCHORS;
        if scores.len() < count || boxes.len() < count * 4 || kps.len() < count * 10 {
            bail!("SCRFD: stride {stride} çıktı boyutu beklenmedik");
        }
        for a in 0..count {
            let score = scores[a];
            if score < SCORE_THRESHOLD {
                continue;
            }
            let loc = a / ANCHORS;
            let (cx, cy) = (((loc % side) * stride) as f32, ((loc / side) * stride) as f32);
            let s = stride as f32;
            let b = &boxes[a * 4..a * 4 + 4];
            let bbox = [(cx - b[0] * s) / scale, (cy - b[1] * s) / scale, (cx + b[2] * s) / scale, (cy + b[3] * s) / scale];
            let k = &kps[a * 10..a * 10 + 10];
            let mut points = [[0f32; 2]; 5];
            for (j, p) in points.iter_mut().enumerate() {
                *p = [(cx + k[j * 2] * s) / scale, (cy + k[j * 2 + 1] * s) / scale];
            }
            found.push(Detection { bbox, score, kps: points });
        }
    }
    Ok(nms(found))
}

fn iou(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let ix = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let iy = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let inter = ix * iy;
    let area = |r: &[f32; 4]| (r[2] - r[0]).max(0.0) * (r[3] - r[1]).max(0.0);
    inter / (area(a) + area(b) - inter + 1e-6)
}

fn nms(mut dets: Vec<Detection>) -> Vec<Detection> {
    dets.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut keep: Vec<Detection> = Vec::new();
    for d in dets {
        if keep.iter().all(|k| iou(&k.bbox, &d.bbox) < NMS_IOU) {
            keep.push(d);
        }
    }
    keep
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_one_anchor_and_suppresses_overlaps() {
        // Build fake outputs with a single confident anchor at stride 8, location (2, 1).
        let mut outs: Vec<Vec<f32>> = STRIDES
            .iter()
            .flat_map(|s| {
                let n = (640 / s) * (640 / s) * ANCHORS;
                [vec![0.0; n], vec![0.0; n * 4], vec![0.0; n * 10]]
            })
            .collect();
        // Reorder into scores×3, boxes×3, kps×3.
        let (mut scores, mut boxes, mut kps) = (vec![], vec![], vec![]);
        for (i, v) in outs.drain(..).enumerate() {
            match i % 3 {
                0 => scores.push(v),
                1 => boxes.push(v),
                _ => kps.push(v),
            }
        }
        let side = 640 / 8;
        let a = (side + 2) * ANCHORS; // y=1, x=2, first anchor
        scores[0][a] = 0.9;
        scores[0][a + 1] = 0.8; // same place, overlapping → suppressed
        for b in [a, a + 1] {
            boxes[0][b * 4..b * 4 + 4].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
        }
        let all: Vec<Vec<f32>> = scores.into_iter().chain(boxes).chain(kps).collect();
        let d = postprocess(&all, 0.5).unwrap();
        assert_eq!(d.len(), 1);
        // Center (16, 8), ±8 px, then /0.5.
        assert_eq!(d[0].bbox, [16.0, 0.0, 48.0, 32.0]);
    }
}
