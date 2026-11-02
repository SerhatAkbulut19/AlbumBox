//! Background engine. Runs on its own OS thread with its own tokio runtime so
//! scans and thumbnail work never slow down the API. The API talks to it only
//! through [`EngineHandle`]: it reads a progress snapshot, subscribes to change
//! notifications and pokes the engine when new jobs are queued.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

use albumbox_core::{jobs, libraries, media, Db};
use serde::Serialize;
use serde_json::json;
use tokio::sync::{watch, Notify};

use albumbox_faces::FaceEngine;

use crate::{detect, faces, scan, tools::Tools};

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub cache_dir: PathBuf,
    pub tools: Tools,
    /// Files hashed/analyzed in parallel. Keep low for USB hard disks.
    pub scan_workers: usize,
    /// Thumbnails generated in parallel (CPU bound).
    pub thumb_workers: usize,
    /// How often drives are checked for plugged/unplugged libraries.
    pub detect_interval: Duration,
    /// Folder with onnxruntime.dll and the face models.
    pub models_dir: PathBuf,
    /// CPU threads for one face analysis.
    pub face_threads: usize,
}

/// Live progress of the running job.
#[derive(Debug, Clone, Default, Serialize)]
pub struct JobProgress {
    pub job_id: i64,
    pub kind: String,
    pub library_id: Option<String>,
    pub library_name: Option<String>,
    /// 'listing' | 'analyzing' | 'thumbs' | 'faces' | 'grouping'
    pub phase: String,
    pub done: u64,
    pub total: u64,
    pub current: String,
    pub listed: u64,
    pub unchanged: u64,
    pub new_media: u64,
    pub existing_media: u64,
    pub missing: u64,
    pub errors: u64,
    pub thumbs_done: u64,
    pub thumbs_failed: u64,
    pub faces_found: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FinishedJob {
    pub status: String,
    pub message: Option<String>,
    pub progress: JobProgress,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EngineSnapshot {
    pub current: Option<JobProgress>,
    pub queued: i64,
    pub last_finished: Option<FinishedJob>,
    /// Bumped whenever library state changes (online/offline, scan finished).
    pub libraries_version: u64,
    pub ffmpeg_available: bool,
    /// Face models and onnxruntime found in the models folder.
    pub faces_available: bool,
}

struct Shared {
    snapshot: RwLock<EngineSnapshot>,
    version: watch::Sender<u64>,
    notify: Notify,
    shutdown: AtomicBool,
    refresh: AtomicBool,
    current_cancel: Mutex<Option<(i64, Arc<AtomicBool>)>>,
    /// Loaded on the first face job and kept for later ones.
    face_engine: tokio::sync::Mutex<Option<Arc<FaceEngine>>>,
    /// Face regrouping: one at a time; requests during a run are coalesced.
    regroup_lock: tokio::sync::Mutex<()>,
    regroup_requested: AtomicBool,
}

impl Shared {
    fn update(&self, f: impl FnOnce(&mut EngineSnapshot)) {
        f(&mut self.snapshot.write().unwrap());
        self.version.send_modify(|v| *v += 1);
    }
}

#[derive(Clone)]
pub struct EngineHandle {
    shared: Arc<Shared>,
    thread: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// The engine's own runtime, for work the API hands over without waiting.
    runtime: tokio::runtime::Handle,
    db: Db,
    pub cache_dir: PathBuf,
}

impl EngineHandle {
    /// Starts the engine thread. `db` should be a pool of its own
    /// ([`Db::open_secondary`]) so the API never waits for a connection.
    pub fn start(db: Db, config: EngineConfig) -> Self {
        let (version, _) = watch::channel(0u64);
        let shared = Arc::new(Shared {
            snapshot: RwLock::new(EngineSnapshot::default()),
            version,
            notify: Notify::new(),
            shutdown: AtomicBool::new(false),
            refresh: AtomicBool::new(false),
            current_cancel: Mutex::new(None),
            face_engine: tokio::sync::Mutex::new(None),
            regroup_lock: tokio::sync::Mutex::new(()),
            regroup_requested: AtomicBool::new(false),
        });
        let cache_dir = config.cache_dir.clone();
        let thread_shared = shared.clone();
        let thread_db = db.clone();
        let (rt_tx, rt_rx) = std::sync::mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("albumbox-engine".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .thread_name("albumbox-engine-rt")
                    .enable_all()
                    .build()
                    .expect("engine runtime");
                let _ = rt_tx.send(rt.handle().clone());
                rt.block_on(Engine { db: thread_db, config, shared: thread_shared }.run());
            })
            .expect("spawn engine thread");
        let runtime = rt_rx.recv().expect("engine runtime handle");
        Self { shared, thread: Arc::new(Mutex::new(Some(thread))), runtime, db, cache_dir }
    }

    /// Recomputes face suggestions and groups on the engine's runtime and
    /// returns immediately. Clients reload when `libraries_version` changes.
    pub fn request_regroup(&self) {
        if self.shared.regroup_requested.swap(true, Ordering::SeqCst) {
            return; // one is already scheduled and will see the latest data
        }
        let (shared, db) = (self.shared.clone(), self.db.clone());
        self.runtime.spawn(async move {
            let _guard = shared.regroup_lock.lock().await;
            shared.regroup_requested.store(false, Ordering::SeqCst);
            if let Err(e) = faces::regroup(&db).await {
                tracing::warn!(error = %format!("{e:#}"), "face regrouping failed");
            }
            shared.update(|s| s.libraries_version += 1);
        });
    }

    pub fn snapshot(&self) -> EngineSnapshot {
        self.shared.snapshot.read().unwrap().clone()
    }

    /// Receiver that changes whenever the snapshot changes.
    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.shared.version.subscribe()
    }

    /// Call after queueing a job.
    pub fn wake(&self) {
        self.shared.notify.notify_one();
    }

    /// Re-check drives now (e.g. the user just plugged in a disk).
    pub fn refresh_libraries(&self) {
        self.shared.refresh.store(true, Ordering::SeqCst);
        self.shared.notify.notify_one();
    }

    /// Cancels the job if it is the one running now. Returns true when signalled.
    pub fn cancel_running(&self, job_id: i64) -> bool {
        match &*self.shared.current_cancel.lock().unwrap() {
            Some((id, flag)) if *id == job_id => {
                flag.store(true, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }

    /// Stops the engine and waits for its thread. Blocking: call from a blocking context.
    pub fn shutdown(&self) {
        self.shared.shutdown.store(true, Ordering::SeqCst);
        if let Some((_, flag)) = &*self.shared.current_cancel.lock().unwrap() {
            flag.store(true, Ordering::SeqCst);
        }
        self.shared.notify.notify_one();
        if let Some(t) = self.thread.lock().unwrap().take() {
            let _ = t.join();
        }
    }
}

/// Context handed to a running job.
pub(crate) struct JobCtx {
    pub db: Db,
    pub config: EngineConfig,
    shared: Arc<Shared>,
    pub job_id: i64,
    pub cancel: Arc<AtomicBool>,
    last_persist: Mutex<Instant>,
}

impl JobCtx {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    pub fn progress(&self, f: impl FnOnce(&mut JobProgress)) {
        let mut persist = None;
        self.shared.update(|s| {
            if let Some(p) = s.current.as_mut() {
                f(p);
                let mut last = self.last_persist.lock().unwrap();
                if last.elapsed() > Duration::from_secs(2) {
                    *last = Instant::now();
                    persist = Some((p.done as i64, p.total as i64));
                }
            }
        });
        if let Some((done, total)) = persist {
            let db = self.db.clone();
            let id = self.job_id;
            tokio::spawn(async move {
                let _ = jobs::set_progress(&db, id, done, total).await;
            });
        }
    }

    pub fn snapshot_progress(&self) -> JobProgress {
        self.shared.snapshot.read().unwrap().current.clone().unwrap_or_default()
    }

    pub fn libraries_changed(&self) {
        self.shared.update(|s| s.libraries_version += 1);
    }

    /// Regroups faces while holding the shared lock (no overlap with API-requested runs).
    pub async fn regroup_faces(&self) -> anyhow::Result<()> {
        let _guard = self.shared.regroup_lock.lock().await;
        faces::regroup(&self.db).await
    }

    /// The face models, loaded on first use.
    pub async fn face_engine(&self) -> anyhow::Result<Arc<FaceEngine>> {
        let mut slot = self.shared.face_engine.lock().await;
        if let Some(e) = slot.as_ref() {
            return Ok(e.clone());
        }
        let (dir, threads) = (self.config.models_dir.clone(), self.config.face_threads);
        let engine = tokio::task::spawn_blocking(move || faces::load_engine(&dir, threads)).await??;
        *slot = Some(engine.clone());
        Ok(engine)
    }
}

struct Engine {
    db: Db,
    config: EngineConfig,
    shared: Arc<Shared>,
}

impl Engine {
    async fn run(self) {
        tracing::info!("engine started");
        let ffmpeg = {
            let tools = self.config.tools.clone();
            tokio::task::spawn_blocking(move || tools.available()).await.unwrap_or(false)
        };
        if !ffmpeg {
            tracing::warn!("ffmpeg/ffprobe not found; video metadata and video thumbnails are disabled");
        }
        let faces_available = faces::models(&self.config.models_dir).is_some();
        if !faces_available {
            tracing::info!(dir = %self.config.models_dir.display(), "face models not found; face recognition is off");
        }
        self.shared.update(|s| {
            s.ffmpeg_available = ffmpeg;
            s.faces_available = faces_available;
        });

        match jobs::requeue_interrupted(&self.db).await {
            Ok(n) if n > 0 => tracing::info!(count = n, "re-queued interrupted jobs"),
            Err(e) => tracing::warn!(error = %e, "requeue failed"),
            _ => {}
        }
        self.detect().await;
        let mut last_detect = Instant::now();

        while !self.shared.shutdown.load(Ordering::SeqCst) {
            if self.shared.refresh.swap(false, Ordering::SeqCst) || last_detect.elapsed() >= self.config.detect_interval {
                self.detect().await;
                last_detect = Instant::now();
            }
            let queued = jobs::count_queued(&self.db).await.unwrap_or(0);
            self.shared.update(|s| s.queued = queued);

            match jobs::next_queued(&self.db).await {
                Ok(Some(job)) => {
                    self.run_job(job).await;
                    continue;
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(error = %e, "job queue read failed"),
            }

            let wait = self.config.detect_interval.saturating_sub(last_detect.elapsed());
            tokio::select! {
                _ = self.shared.notify.notified() => {}
                _ = tokio::time::sleep(wait) => {}
            }
        }
        tracing::info!("engine stopped");
    }

    /// Finds libraries on the mounted drives and updates their online state.
    async fn detect(&self) {
        let libs = match libraries::list(&self.db).await {
            Ok(l) => l,
            Err(e) => return tracing::warn!(error = %e, "library list failed"),
        };
        if libs.is_empty() {
            return;
        }
        let found = tokio::task::spawn_blocking(move || {
            let drives = detect::mounted_drives();
            libs.into_iter()
                .map(|l| {
                    let root = detect::locate(&l.id, &l.root_subpath, l.last_seen_root.as_deref(), &drives);
                    (l, root.map(|r| detect::root_string(&r)))
                })
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();

        let mut changed = false;
        let mut came_online = false;
        for (lib, root) in found {
            match libraries::set_location(&self.db, &lib.id, root.as_deref()).await {
                Ok(true) => {
                    changed = true;
                    came_online |= root.is_some() && !lib.is_online;
                    match &root {
                        Some(r) => tracing::info!(library = %lib.name, root = %r, "library online"),
                        None => tracing::info!(library = %lib.name, "library offline"),
                    }
                }
                Ok(false) => {}
                Err(e) => tracing::warn!(error = %e, "library update failed"),
            }
        }
        if changed {
            self.shared.update(|s| s.libraries_version += 1);
        }
        // A disk came back: finish thumbnails that were waiting for it.
        if came_online && media::count_pending_thumbs(&self.db).await.unwrap_or(0) > 0 {
            let _ = jobs::enqueue(&self.db, "thumbs", &json!({})).await;
        }
    }

    async fn run_job(&self, job: jobs::Job) {
        let cancel = Arc::new(AtomicBool::new(false));
        *self.shared.current_cancel.lock().unwrap() = Some((job.id, cancel.clone()));
        if let Err(e) = jobs::mark_running(&self.db, job.id).await {
            tracing::warn!(error = %e, "mark running failed");
        }
        let library_id = job.payload.get("library_id").and_then(|v| v.as_str()).map(str::to_string);
        let library_name = match &library_id {
            Some(id) => libraries::get(&self.db, id).await.ok().flatten().map(|l| l.name),
            None => None,
        };
        let queued = jobs::count_queued(&self.db).await.unwrap_or(0);
        self.shared.update(|s| {
            s.queued = queued;
            s.current = Some(JobProgress {
                job_id: job.id,
                kind: job.kind.clone(),
                library_id: library_id.clone(),
                library_name,
                ..Default::default()
            })
        });

        let ctx = JobCtx {
            db: self.db.clone(),
            config: self.config.clone(),
            shared: self.shared.clone(),
            job_id: job.id,
            cancel,
            last_persist: Mutex::new(Instant::now()),
        };
        tracing::info!(job = job.id, kind = %job.kind, "job started");
        let result = match (job.kind.as_str(), library_id) {
            ("scan", Some(lib)) => scan::run_scan(&ctx, &lib).await,
            ("thumbs", _) => scan::run_thumbs(&ctx).await.map(|_| ()),
            ("faces", _) => faces::run_faces(&ctx).await,
            _ => Err(anyhow::anyhow!("unknown job kind '{}'", job.kind)),
        };

        let progress = ctx.snapshot_progress();
        let (status, message) = if ctx.cancelled() {
            if self.shared.shutdown.load(Ordering::SeqCst) { ("queued", None) } else { ("cancelled", None) }
        } else {
            match &result {
                Ok(()) => ("done", None),
                Err(e) => ("failed", Some(format!("{e:#}"))),
            }
        };
        match &result {
            Err(e) if status == "failed" => tracing::warn!(job = job.id, error = %format!("{e:#}"), "job failed"),
            _ => tracing::info!(job = job.id, status, "job finished"),
        }
        let _ = jobs::set_progress(&self.db, job.id, progress.done as i64, progress.total as i64).await;
        let _ = jobs::finish(&self.db, job.id, status, message.as_deref()).await;

        // New thumbnails → look for faces next (when the models are installed).
        let faces_on = self.shared.snapshot.read().unwrap().faces_available;
        if status == "done" && faces_on && matches!(job.kind.as_str(), "scan" | "thumbs") {
            if albumbox_core::faces::count_pending(&self.db).await.unwrap_or(0) > 0 {
                let _ = jobs::enqueue(&self.db, "faces", &json!({})).await;
            }
        }

        *self.shared.current_cancel.lock().unwrap() = None;
        self.shared.update(|s| {
            s.current = None;
            s.libraries_version += 1;
            s.last_finished = Some(FinishedJob { status: status.into(), message, progress });
        });
    }
}
