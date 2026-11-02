//! Scan job: list files → skip unchanged → hash + metadata → mark missing →
//! thumbnails.

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::UNIX_EPOCH,
};

use albumbox_core::{
    libraries,
    media::{self, MediaKind, UpsertOutcome},
};
use anyhow::{bail, Context};
use futures::StreamExt;
use time::{macros::format_description, OffsetDateTime};
use walkdir::WalkDir;

use crate::{analyze, detect, engine::JobCtx, kinds, thumbs};

#[derive(Debug, Clone)]
pub struct Entry {
    /// Relative to the library root, '\' separated.
    pub rel: String,
    pub folder: String,
    pub size: i64,
    pub mtime: i64,
    pub kind: MediaKind,
}

/// Millisecond UTC timestamp; fixed width so text comparison orders correctly.
fn scan_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(format_description!("[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"))
        .unwrap_or_default()
}

fn rel_string(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    Some(rel.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("\\"))
}

/// Lists media files under `root`. Blocking.
pub fn list_files(root: &Path, cancel: &AtomicBool, listed: &AtomicU64) -> Vec<Entry> {
    let mut out = Vec::new();
    let walker = WalkDir::new(root).follow_links(false).into_iter().filter_entry(|e| {
        e.depth() == 0 || !e.file_type().is_dir() || !kinds::is_skipped_dir(&e.file_name().to_string_lossy())
    });
    for entry in walker {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::debug!(error = %e, "walk error");
                continue;
            }
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let Some(kind) = kinds::kind_of(entry.path()) else { continue };
        let Ok(meta) = entry.metadata() else { continue };
        let Some(rel) = rel_string(root, entry.path()) else { continue };
        let folder = rel.rsplit_once('\\').map(|(f, _)| f.to_string()).unwrap_or_default();
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        out.push(Entry { rel, folder, size: meta.len() as i64, mtime, kind });
        listed.fetch_add(1, Ordering::Relaxed);
    }
    out
}

fn abs_path(root: &Path, rel: &str) -> PathBuf {
    rel.split('\\').fold(root.to_path_buf(), |p, s| p.join(s))
}

pub(crate) async fn run_scan(ctx: &JobCtx, library_id: &str) -> anyhow::Result<()> {
    let lib = libraries::get(&ctx.db, library_id).await?.context("library not found")?;
    let lib_id = lib.id.clone();
    let root = tokio::task::spawn_blocking(move || {
        let drives = detect::mounted_drives();
        detect::locate(&lib.id, &lib.root_subpath, lib.last_seen_root.as_deref(), &drives)
    })
    .await?;
    let Some(root) = root else {
        libraries::set_location(&ctx.db, &lib_id, None).await?;
        ctx.libraries_changed();
        bail!("library_offline");
    };
    libraries::set_location(&ctx.db, &lib_id, Some(&detect::root_string(&root))).await?;
    let scan_started = scan_timestamp();

    // 1) Listing
    ctx.progress(|p| p.phase = "listing".into());
    let listed = std::sync::Arc::new(AtomicU64::new(0));
    let entries = {
        let (root, cancel, counter) = (root.clone(), ctx.cancel.clone(), listed.clone());
        let mut task = tokio::task::spawn_blocking(move || list_files(&root, &cancel, &counter));
        loop {
            tokio::select! {
                r = &mut task => break r?,
                _ = tokio::time::sleep(std::time::Duration::from_millis(300)) => {
                    let n = listed.load(Ordering::Relaxed);
                    ctx.progress(|p| p.listed = n);
                }
            }
        }
    };
    if ctx.cancelled() {
        return Ok(());
    }

    // 2) Unchanged files only get their "seen" time refreshed.
    let known = media::known_files(&ctx.db, &lib_id).await?;
    let mut unchanged = Vec::new();
    let mut work = Vec::new();
    for e in entries.iter() {
        match known.get(&e.rel) {
            Some(k) if k.size == e.size && k.mtime == e.mtime => unchanged.push(k.id),
            _ => work.push(e.clone()),
        }
    }
    media::touch_files(&ctx.db, &unchanged, &scan_started).await?;
    let total = work.len() as u64;
    ctx.progress(|p| {
        p.phase = "analyzing".into();
        p.listed = entries.len() as u64;
        p.unchanged = unchanged.len() as u64;
        p.total = total;
    });

    // 3) Hash + metadata, a few files at a time.
    let tools = ctx.config.tools.clone();
    let mut stream = futures::stream::iter(work)
        .map(|e| {
            let path = abs_path(&root, &e.rel);
            let tools = tools.clone();
            async move {
                let (kind, mtime) = (e.kind, e.mtime);
                let r = tokio::task::spawn_blocking(move || analyze::analyze(&path, kind, mtime, &tools)).await;
                (e, r)
            }
        })
        .buffer_unordered(ctx.config.scan_workers.max(1));

    while let Some((e, result)) = stream.next().await {
        if ctx.cancelled() {
            return Ok(());
        }
        let outcome = match result {
            Ok(Ok(m)) => {
                media::upsert_file(&ctx.db, &lib_id, &e.rel, &e.folder, e.size, e.mtime, &scan_started, &m)
                    .await
                    .map_err(anyhow::Error::from)
            }
            Ok(Err(err)) => Err(err),
            Err(join) => Err(join.into()),
        };
        ctx.progress(|p| {
            p.done += 1;
            p.current = e.rel.clone();
            match &outcome {
                Ok(UpsertOutcome::NewMedia) => p.new_media += 1,
                Ok(UpsertOutcome::ExistingMedia) => p.existing_media += 1,
                Err(_) => p.errors += 1,
            }
        });
        if let Err(err) = outcome {
            tracing::warn!(file = %e.rel, error = %format!("{err:#}"), "file skipped");
        }
    }
    drop(stream);

    // 4) Files gone from the disk (deleted, moved, renamed) are flagged, never deleted.
    let missing = media::mark_missing(&ctx.db, &lib_id, &scan_started).await?;
    libraries::set_scanned_now(&ctx.db, &lib_id).await?;
    ctx.progress(|p| {
        p.missing = missing;
        p.current.clear();
    });
    ctx.libraries_changed();

    // 5) Thumbnails for everything new.
    run_thumbs(ctx).await?;
    Ok(())
}

/// Generates missing thumbnails for all reachable media. Returns how many were made.
pub(crate) async fn run_thumbs(ctx: &JobCtx) -> anyhow::Result<u64> {
    let total = media::count_pending_thumbs(&ctx.db).await? as u64;
    ctx.progress(|p| {
        p.phase = "thumbs".into();
        p.done = 0;
        p.total = total;
        p.current.clear();
    });
    let cache_dir = ctx.config.cache_dir.clone();
    let mut made = 0;
    loop {
        if ctx.cancelled() {
            break;
        }
        let batch = media::pending_thumbs(&ctx.db, 64).await?;
        if batch.is_empty() {
            break;
        }
        let tools = ctx.config.tools.clone();
        let mut stream = futures::stream::iter(batch)
            .map(|t| {
                let (tools, cache_dir) = (tools.clone(), cache_dir.clone());
                async move {
                    let src = abs_path(Path::new(&t.root), &t.rel_path);
                    let (kind, duration, hash) = (t.kind, t.duration_ms, t.hash.clone());
                    let r = tokio::task::spawn_blocking(move || {
                        thumbs::generate(&src, kind, duration, &cache_dir, &hash, &tools)
                    })
                    .await;
                    (t, r)
                }
            })
            .buffer_unordered(ctx.config.thumb_workers.max(1));

        while let Some((t, result)) = stream.next().await {
            let ok = match result {
                Ok(Ok((w, h))) => {
                    media::set_thumb_state(&ctx.db, t.media_id, 1, Some((w as i64, h as i64))).await?;
                    made += 1;
                    true
                }
                Ok(Err(e)) => {
                    tracing::warn!(file = %t.rel_path, error = %format!("{e:#}"), "thumbnail failed");
                    media::set_thumb_state(&ctx.db, t.media_id, -1, None).await?;
                    false
                }
                Err(e) => {
                    tracing::warn!(file = %t.rel_path, error = %e, "thumbnail task failed");
                    media::set_thumb_state(&ctx.db, t.media_id, -1, None).await?;
                    false
                }
            };
            ctx.progress(|p| {
                p.done += 1;
                p.current = t.rel_path.clone();
                if ok { p.thumbs_done += 1 } else { p.thumbs_failed += 1 }
            });
            if ctx.cancelled() {
                break;
            }
        }
    }
    ctx.progress(|p| p.current.clear());
    Ok(made)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_media_and_skips_system_folders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for p in ["Köy 2022/IMG_1.jpg", "Köy 2022/Nişan/VID_1.mp4", "$RECYCLE.BIN/x.jpg", ".hidden/y.jpg", "notlar.txt"] {
            let path = root.join(p);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"x").unwrap();
        }
        let mut entries = list_files(root, &AtomicBool::new(false), &AtomicU64::new(0));
        entries.sort_by(|a, b| a.rel.cmp(&b.rel));
        let rels: Vec<_> = entries.iter().map(|e| (e.rel.as_str(), e.folder.as_str())).collect();
        assert_eq!(rels, vec![("Köy 2022\\IMG_1.jpg", "Köy 2022"), ("Köy 2022\\Nişan\\VID_1.mp4", "Köy 2022\\Nişan")]);
    }
}
