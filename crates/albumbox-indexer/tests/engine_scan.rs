//! End-to-end: a temp folder plays the external disk.

use std::{path::Path, time::Duration};

use albumbox_core::{jobs, libraries, media, Db};
use albumbox_indexer::{detect, marker, thumbs, tools::Tools, EngineConfig, EngineHandle};
use image::{Rgb, RgbImage};
use serde_json::json;

fn photo(path: &Path, seed: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    RgbImage::from_fn(640, 480, |x, y| Rgb([seed, (x % 255) as u8, (y % 255) as u8])).save(path).unwrap();
}

fn video(path: &Path, tools: &Tools) -> bool {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut cmd = albumbox_indexer::tools::command(&tools.ffmpeg);
    cmd.args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc=duration=2:size=320x240:rate=10"])
        .args(["-metadata", "creation_time=2023-04-21T09:30:00Z", "-pix_fmt", "yuv420p"])
        .arg(path);
    albumbox_indexer::tools::run(cmd).is_ok()
}

async fn run_scan(db: &Db, engine: &EngineHandle, library_id: &str) -> jobs::Job {
    let id = jobs::enqueue(db, "scan", &json!({ "library_id": library_id })).await.unwrap();
    engine.wake();
    for _ in 0..600 {
        let job = jobs::recent(db, 20).await.unwrap().into_iter().find(|j| j.id == id).unwrap();
        if !matches!(job.status.as_str(), "queued" | "running") {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("scan did not finish");
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_rescan_and_offline_detection() {
    let tmp = tempfile::tempdir().unwrap();
    let disk = tmp.path().join("disk");
    let root = disk.join("Aile");
    photo(&root.join("Köy 2022").join("IMG_20220514_132210.jpg"), 10);
    photo(&root.join("Köy 2022").join("Nişan").join("IMG_20220515_101010.jpg"), 20);
    photo(&root.join("Ramazan").join("foto.png"), 30);
    std::fs::copy(root.join("Köy 2022").join("IMG_20220514_132210.jpg"), {
        std::fs::create_dir_all(root.join("Yedek")).unwrap();
        root.join("Yedek").join("kopya.jpg")
    })
    .unwrap();
    let tools = Tools::default();
    let has_video = tools.available() && video(&root.join("Ramazan").join("bayram.mp4"), &tools);

    let db = Db::open(&tmp.path().join("test.db")).await.unwrap();
    let m = marker::create(&root, "Aile").unwrap();
    let (_, subpath) = detect::split_drive(&root);
    libraries::insert(&db, &m.library_id, &m.name, &subpath, &root.to_string_lossy()).await.unwrap();

    let cache = tmp.path().join("cache");
    let engine = EngineHandle::start(
        db.clone(),
        EngineConfig {
            cache_dir: cache.clone(),
            tools: tools.clone(),
            scan_workers: 2,
            thumb_workers: 2,
            detect_interval: Duration::from_secs(3600),
            models_dir: tmp.path().join("no-models"),
            face_threads: 1,
        },
    );

    // First scan: 4 photo files (one is a copy) + optional video.
    let job = run_scan(&db, &engine, &m.library_id).await;
    assert_eq!(job.status, "done", "{:?}", job.message);
    let lib = libraries::get(&db, &m.library_id).await.unwrap().unwrap();
    let extra = has_video as i64;
    assert_eq!(lib.file_count, 4 + extra);
    assert_eq!(lib.media_count, 3 + extra, "the copy shares one media row");
    assert_eq!(media::count_pending_thumbs(&db).await.unwrap(), 0);

    let rows: Vec<(String, i64, String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT hash, thumb_ready, kind, taken_at, taken_at_source FROM media ORDER BY id")
            .fetch_all(&db.pool)
            .await
            .unwrap();
    for (hash, ready, kind, _, _) in &rows {
        assert_eq!(*ready, 1, "{kind} thumbnail");
        assert!(thumbs::thumb_path(&cache, hash, 256).exists());
        assert!(thumbs::thumb_path(&cache, hash, 1024).exists());
    }
    assert!(rows.iter().any(|r| r.3.as_deref() == Some("2022-05-14T13:22:10") && r.4.as_deref() == Some("filename")));
    if has_video {
        let v = rows.iter().find(|r| r.2 == "video").unwrap();
        assert_eq!(v.4.as_deref(), Some("video_meta"));
        assert!(v.3.as_deref().unwrap().starts_with("2023-04-21T"));
    }

    // Rename a folder and rescan: content is recognised, old paths become missing.
    std::fs::rename(root.join("Köy 2022"), root.join("Köy Düğünü 2022")).unwrap();
    let job = run_scan(&db, &engine, &m.library_id).await;
    assert_eq!(job.status, "done");
    let lib = libraries::get(&db, &m.library_id).await.unwrap().unwrap();
    assert_eq!(lib.media_count, 3 + extra, "no new media for moved files");
    assert_eq!(lib.missing_count, 2);
    let folders = libraries::folders(&db, &m.library_id).await.unwrap();
    assert!(folders.iter().any(|f| f.folder_rel == "Köy Düğünü 2022\\Nişan"));

    // "Unplug" the disk: the library goes offline and scans refuse to run.
    std::fs::rename(&disk, tmp.path().join("disk-unplugged")).unwrap();
    let job = run_scan(&db, &engine, &m.library_id).await;
    assert_eq!(job.status, "failed");
    assert!(job.message.unwrap_or_default().contains("library_offline"));
    assert!(!libraries::get(&db, &m.library_id).await.unwrap().unwrap().is_online);

    // Plug it back in: detection brings it online again.
    std::fs::rename(tmp.path().join("disk-unplugged"), &disk).unwrap();
    engine.refresh_libraries();
    let mut online = false;
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if libraries::get(&db, &m.library_id).await.unwrap().unwrap().is_online {
            online = true;
            break;
        }
    }
    assert!(online, "library detected again");

    let e = engine.clone();
    tokio::task::spawn_blocking(move || e.shutdown()).await.unwrap();
}
