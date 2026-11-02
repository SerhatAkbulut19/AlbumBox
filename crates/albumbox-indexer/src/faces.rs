//! Face job: detect faces on the 1024 px thumbnails (already oriented, also
//! available while the disk is unplugged), then group them.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use albumbox_core::{
    faces::{self, Assignment, NewFace},
    Db,
};
use albumbox_faces::{cluster, from_blob, to_blob, FaceEngine, ModelFiles};
use anyhow::Context;
use image::codecs::jpeg::JpegEncoder;

use crate::{engine::JobCtx, thumbs};

pub fn crop_path(cache_dir: &Path, face_id: i64) -> PathBuf {
    cache_dir.join("faces").join(format!("{:02}", face_id % 100)).join(format!("{face_id}.jpg"))
}

pub fn models(dir: &Path) -> Option<ModelFiles> {
    ModelFiles::find(dir)
}

pub(crate) async fn run_faces(ctx: &JobCtx) -> anyhow::Result<()> {
    let engine = ctx.face_engine().await?;
    let total = faces::count_pending(&ctx.db).await? as u64;
    ctx.progress(|p| {
        p.phase = "faces".into();
        p.total = total;
        p.done = 0;
    });
    let cache = ctx.config.cache_dir.clone();
    loop {
        if ctx.cancelled() {
            return Ok(());
        }
        let batch = faces::pending_media(&ctx.db, 32).await?;
        if batch.is_empty() {
            break;
        }
        for (media_id, hash) in batch {
            if ctx.cancelled() {
                return Ok(());
            }
            let thumb = thumbs::thumb_path(&cache, &hash, 1024);
            let engine = engine.clone();
            let result = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<(NewFace, Vec<u8>)>> {
                let img = image::open(&thumb).with_context(|| format!("open {}", thumb.display()))?.to_rgb8();
                let mut out = Vec::new();
                for f in engine.analyze(&img)? {
                    let mut jpeg = Vec::new();
                    JpegEncoder::new_with_quality(&mut jpeg, 85).encode_image(&f.crop)?;
                    out.push((NewFace { bbox: f.bbox, score: f.score, embedding: to_blob(&f.embedding) }, jpeg));
                }
                Ok(out)
            })
            .await?;
            let found = match result {
                Ok(found) => {
                    let (rows, crops): (Vec<_>, Vec<_>) = found.into_iter().unzip();
                    let ids = faces::save_media_faces(&ctx.db, media_id, &rows).await?;
                    for (id, jpeg) in ids.iter().zip(crops) {
                        let path = crop_path(&cache, *id);
                        if let Some(dir) = path.parent() {
                            tokio::fs::create_dir_all(dir).await?;
                        }
                        tokio::fs::write(&path, jpeg).await?;
                    }
                    ids.len() as u64
                }
                Err(e) => {
                    tracing::warn!(media_id, error = %format!("{e:#}"), "face analysis failed");
                    faces::mark_failed(&ctx.db, media_id).await?;
                    0
                }
            };
            ctx.progress(|p| {
                p.done += 1;
                p.faces_found += found;
            });
        }
    }
    ctx.progress(|p| p.phase = "grouping".into());
    ctx.regroup_faces().await?;
    Ok(())
}

/// Recomputes person averages, suggestions for open faces and the unnamed groups.
pub async fn regroup(db: &Db) -> anyhow::Result<()> {
    let confirmed = faces::confirmed_faces(db).await?;
    let open = faces::open_faces(db).await?;
    let (assignments, persons) = tokio::task::spawn_blocking(move || {
        // Person averages from confirmed faces.
        let mut persons: Vec<(i64, Vec<f32>, i64, Option<i64>)> = Vec::new();
        let mut i = 0;
        while i < confirmed.len() {
            let tag = confirmed[i].0;
            let group: Vec<_> = confirmed[i..].iter().take_while(|r| r.0 == tag).collect();
            let vectors: Vec<Vec<f32>> = group.iter().map(|r| from_blob(&r.3)).collect();
            if let Some(c) = cluster::centroid(vectors.iter().map(Vec::as_slice)) {
                let cover = group.iter().max_by(|a, b| a.2.total_cmp(&b.2)).map(|r| r.1);
                persons.push((tag, c, group.len() as i64, cover));
            }
            i += group.len();
        }
        let centers: Vec<(i64, Vec<f32>)> = persons.iter().map(|p| (p.0, p.1.clone())).collect();

        // Suggestions first; the rest is grouped.
        let embeddings: Vec<Vec<f32>> = open.iter().map(|(_, e)| from_blob(e)).collect();
        let mut assignments = Vec::with_capacity(open.len());
        let mut rest: Vec<usize> = Vec::new();
        for (i, (id, _)) in open.iter().enumerate() {
            match cluster::best_person(&embeddings[i], &centers, cluster::SUGGEST_THRESHOLD) {
                Some((p, _)) => assignments.push(Assignment { face_id: *id, suggested: Some(p), cluster: None }),
                None => rest.push(i),
            }
        }
        let refs: Vec<&[f32]> = rest.iter().map(|&i| embeddings[i].as_slice()).collect();
        let labels = cluster::cluster(&refs, cluster::CLUSTER_THRESHOLD);
        for (&i, label) in rest.iter().zip(labels) {
            assignments.push(Assignment { face_id: open[i].0, suggested: None, cluster: Some(label as i64 + 1) });
        }
        let persons: Vec<(i64, Vec<u8>, i64, Option<i64>)> =
            persons.into_iter().map(|(t, c, n, cover)| (t, to_blob(&c), n, cover)).collect();
        (assignments, persons)
    })
    .await?;
    faces::apply_grouping(db, &assignments, &persons).await?;
    Ok(())
}

/// Loads the models once; shared by every face job.
pub(crate) fn load_engine(dir: &Path, threads: usize) -> anyhow::Result<Arc<FaceEngine>> {
    let files = models(dir).ok_or_else(|| anyhow::anyhow!("face_models_missing"))?;
    Ok(Arc::new(FaceEngine::load(&files, threads)?))
}
