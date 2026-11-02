//! `albumbox.toml` next to the executable. Relative paths resolve against the
//! executable directory, so the service works regardless of working directory.

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use albumbox_indexer::{tools::Tools, EngineConfig};
use anyhow::Context;
use serde::{Deserialize, Serialize};

pub const CONFIG_FILE: &str = "albumbox.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Listen address, e.g. "0.0.0.0:8080".
    pub bind: String,
    /// Database, thumbnail cache, models and logs.
    pub data_dir: PathBuf,
    /// Built web UI (contains index.html).
    pub ui_dir: PathBuf,
    /// Reject requests from outside the local network.
    pub lan_only: bool,
    /// Session lifetime without "beni hatırla".
    pub session_hours: i64,
    /// Session lifetime with "beni hatırla".
    pub remember_days: i64,
    /// tracing filter, e.g. "info" or "albumbox_api=debug,info".
    pub log_level: String,
    /// Folder holding ffmpeg.exe and ffprobe.exe. Empty: `<exe dir>\ffmpeg` if
    /// present, otherwise PATH.
    pub ffmpeg_dir: PathBuf,
    /// Files hashed in parallel during a scan (USB hard disks like 2).
    pub scan_workers: usize,
    /// Thumbnails generated in parallel.
    pub thumb_workers: usize,
    /// Seconds between drive checks for plugged/unplugged disks.
    pub detect_interval_secs: u64,
    /// Folder with onnxruntime.dll and the face models (see scriptsyuz-modeli-indir.ps1).
    pub models_dir: PathBuf,
    /// CPU threads used by face recognition.
    pub face_threads: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0:8080".into(),
            data_dir: "data".into(),
            ui_dir: "ui".into(),
            lan_only: true,
            session_hours: 12,
            remember_days: 30,
            log_level: "info".into(),
            ffmpeg_dir: PathBuf::new(),
            scan_workers: 2,
            thumb_workers: std::thread::available_parallelism().map(|n| (n.get() / 2).clamp(1, 4)).unwrap_or(2),
            detect_interval_secs: 10,
            models_dir: "models".into(),
            face_threads: std::thread::available_parallelism().map(|n| (n.get() / 2).clamp(1, 4)).unwrap_or(2),
        }
    }
}

pub fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

impl Config {
    /// Loads the config file, writing a default one on first run.
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let base = exe_dir();
        let path = path.map(Path::to_path_buf).unwrap_or_else(|| base.join(CONFIG_FILE));
        let mut config = if path.exists() {
            let text = std::fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
            toml::from_str::<Config>(&text).with_context(|| format!("parse {}", path.display()))?
        } else {
            let config = Config::default();
            // Best effort: the directory may be read-only.
            let _ = std::fs::write(&path, toml::to_string_pretty(&config)?);
            config
        };
        let config_dir = path.parent().map(Path::to_path_buf).unwrap_or(base);
        config.data_dir = absolute(&config_dir, &config.data_dir);
        if !config.ffmpeg_dir.as_os_str().is_empty() {
            config.ffmpeg_dir = absolute(&config_dir, &config.ffmpeg_dir);
        }
        config.ui_dir = resolve_ui_dir(&absolute(&config_dir, &config.ui_dir));
        config.models_dir = resolve_models_dir(&absolute(&config_dir, &config.models_dir));
        Ok(config)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("albumbox.db")
    }

    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.data_dir.join("cache")
    }

    pub fn tools(&self) -> Tools {
        let exe = |dir: &Path, name: &str| dir.join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let dir = if !self.ffmpeg_dir.as_os_str().is_empty() {
            Some(self.ffmpeg_dir.clone())
        } else {
            Some(exe_dir().join("ffmpeg")).filter(|d| exe(d, "ffmpeg").exists())
        };
        match dir {
            Some(d) => Tools { ffmpeg: exe(&d, "ffmpeg"), ffprobe: exe(&d, "ffprobe") },
            None => Tools::default(),
        }
    }

    pub fn engine(&self) -> EngineConfig {
        EngineConfig {
            cache_dir: self.cache_dir(),
            tools: self.tools(),
            scan_workers: self.scan_workers.max(1),
            thumb_workers: self.thumb_workers.max(1),
            detect_interval: Duration::from_secs(self.detect_interval_secs.max(2)),
            models_dir: self.models_dir.clone(),
            face_threads: self.face_threads.max(1),
        }
    }
}

fn absolute(base: &Path, p: &Path) -> PathBuf {
    if p.is_absolute() { p.to_path_buf() } else { base.join(p) }
}

/// Uses the configured directory when it holds a UI build; otherwise, during
/// development, looks for `ui/dist` in an ancestor of the executable.
fn resolve_ui_dir(configured: &Path) -> PathBuf {
    if configured.join("index.html").exists() {
        return configured.to_path_buf();
    }
    exe_dir()
        .ancestors()
        .map(|dir| dir.join("ui").join("dist"))
        .find(|dir| dir.join("index.html").exists())
        .unwrap_or_else(|| configured.to_path_buf())
}

/// Uses the configured folder when it holds the models; otherwise, during
/// development, looks for `models` in an ancestor of the executable.
fn resolve_models_dir(configured: &Path) -> PathBuf {
    if albumbox_indexer::faces::models(configured).is_some() {
        return configured.to_path_buf();
    }
    exe_dir()
        .ancestors()
        .map(|dir| dir.join("models"))
        .find(|dir| albumbox_indexer::faces::models(dir).is_some())
        .unwrap_or_else(|| configured.to_path_buf())
}
