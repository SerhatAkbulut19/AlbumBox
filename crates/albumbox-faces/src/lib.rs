//! Local face detection and recognition (InsightFace models on ONNX Runtime).
//!
//! - detection: SCRFD (`det_10g.onnx` / `det_500m.onnx`) → boxes + 5 landmarks
//! - alignment: similarity transform of the landmarks onto the ArcFace template
//! - recognition: ArcFace (`w600k_r50.onnx` / `w600k_mbf.onnx`) → 512-d embedding
//!
//! Everything runs on the CPU of this machine; no image ever leaves it.
//! `onnxruntime.dll` is loaded at runtime from the models folder.

mod align;
pub mod cluster;
mod scrfd;

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use anyhow::{anyhow, Context};
use image::RgbImage;
use ndarray::Array4;
use ort::{
    environment::Environment,
    session::{builder::GraphOptimizationLevel, Session},
    value::TensorRef,
};

pub const EMBEDDING_DIM: usize = 512;
/// Faces smaller than this (in pixels of the analysed image) are ignored.
pub const MIN_FACE_PX: f32 = 28.0;
pub const MIN_SCORE: f32 = 0.6;

/// Files needed in the models folder.
#[derive(Debug, Clone)]
pub struct ModelFiles {
    pub runtime: PathBuf,
    pub detector: PathBuf,
    pub recognizer: PathBuf,
}

impl ModelFiles {
    /// Looks for `onnxruntime.dll`, a `det_*.onnx` and a `w600k_*.onnx`
    /// (preferring the larger, more accurate models).
    pub fn find(dir: &Path) -> Option<Self> {
        let runtime = dir.join(if cfg!(windows) { "onnxruntime.dll" } else { "libonnxruntime.so" });
        let pick = |names: &[&str]| names.iter().map(|n| dir.join(n)).find(|p| p.exists());
        Some(Self {
            detector: pick(&["det_10g.onnx", "det_2.5g.onnx", "det_500m.onnx"])?,
            recognizer: pick(&["w600k_r50.onnx", "w600k_mbf.onnx"])?,
            runtime: runtime.exists().then_some(runtime)?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct DetectedFace {
    /// Box relative to the image (0..1): x, y, width, height.
    pub bbox: [f32; 4],
    pub score: f32,
    /// L2-normalized embedding.
    pub embedding: Vec<f32>,
    /// Square crop around the face for the UI (160×160).
    pub crop: RgbImage,
}

pub struct FaceEngine {
    _env: Environment,
    detector: Mutex<Session>,
    recognizer: Mutex<Session>,
}

impl FaceEngine {
    pub fn load(files: &ModelFiles, threads: usize) -> anyhow::Result<Self> {
        let env = ort::init_from(&files.runtime)
            .map_err(|e| anyhow!("onnxruntime yüklenemedi ({}): {e}", files.runtime.display()))?
            .with_name("albumbox")
            .build()
            .map_err(|e| anyhow!("onnxruntime ortamı: {e}"))?;
        let session = |path: &Path| -> anyhow::Result<Session> {
            Session::builder(&env)
                .map_err(|e| anyhow!("{e}"))?
                .with_intra_threads(threads.max(1))
                .map_err(|e| anyhow!("{e}"))?
                .with_optimization_level(GraphOptimizationLevel::Level3)
                .map_err(|e| anyhow!("{e}"))?
                .commit_from_file(path)
                .map_err(|e| anyhow!("{}: {e}", path.display()))
        };
        let detector = session(&files.detector)?;
        let recognizer = session(&files.recognizer)?;
        tracing::info!(detector = %files.detector.display(), recognizer = %files.recognizer.display(), "face models loaded");
        Ok(Self { detector: Mutex::new(detector), recognizer: Mutex::new(recognizer), _env: env })
    }

    /// Finds faces in an (already oriented) image and computes their embeddings.
    pub fn analyze(&self, img: &RgbImage) -> anyhow::Result<Vec<DetectedFace>> {
        let (w, h) = (img.width() as f32, img.height() as f32);
        let detections = {
            let (input, scale) = scrfd::preprocess(img);
            let mut det = self.detector.lock().map_err(|_| anyhow!("detector poisoned"))?;
            let outputs = det.run(ort::inputs![TensorRef::from_array_view(&input)?])?;
            let mut tensors = Vec::with_capacity(outputs.len());
            for i in 0..outputs.len() {
                tensors.push(outputs[i].try_extract_array::<f32>()?.iter().copied().collect::<Vec<f32>>());
            }
            scrfd::postprocess(&tensors, scale).context("SCRFD çıktısı")?
        };

        let mut faces = Vec::new();
        for d in detections {
            let [x1, y1, x2, y2] = d.bbox;
            if d.score < MIN_SCORE || (x2 - x1).min(y2 - y1) < MIN_FACE_PX {
                continue;
            }
            let aligned = align::align_arcface(img, &d.kps);
            let embedding = self.embed(&aligned)?;
            let crop = align::square_crop(img, d.bbox, 160);
            faces.push(DetectedFace {
                bbox: [
                    (x1 / w).clamp(0.0, 1.0),
                    (y1 / h).clamp(0.0, 1.0),
                    ((x2 - x1) / w).clamp(0.0, 1.0),
                    ((y2 - y1) / h).clamp(0.0, 1.0),
                ],
                score: d.score,
                embedding,
                crop,
            });
        }
        Ok(faces)
    }

    fn embed(&self, face: &RgbImage) -> anyhow::Result<Vec<f32>> {
        let mut input = Array4::<f32>::zeros((1, 3, 112, 112));
        for (x, y, p) in face.enumerate_pixels() {
            for c in 0..3 {
                input[[0, c, y as usize, x as usize]] = (p[c] as f32 - 127.5) / 127.5;
            }
        }
        let mut rec = self.recognizer.lock().map_err(|_| anyhow!("recognizer poisoned"))?;
        let outputs = rec.run(ort::inputs![TensorRef::from_array_view(&input)?])?;
        let mut v: Vec<f32> = outputs[0].try_extract_array::<f32>()?.iter().copied().collect();
        if v.len() != EMBEDDING_DIM {
            return Err(anyhow!("beklenmeyen embedding boyutu: {}", v.len()));
        }
        cluster::normalize(&mut v);
        Ok(v)
    }
}

/// Embedding ↔ BLOB (little-endian f32).
pub fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

pub fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}
