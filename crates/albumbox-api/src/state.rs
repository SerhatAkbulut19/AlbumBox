use std::sync::Arc;

use albumbox_core::Db;
use albumbox_indexer::EngineHandle;

use crate::{config::Config, limiter::LoginLimiter};

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub config: Arc<Config>,
    pub engine: EngineHandle,
    pub login_limiter: Arc<LoginLimiter>,
    /// Serializes first-run setup so two admins cannot be created concurrently.
    pub setup_lock: Arc<tokio::sync::Mutex<()>>,
}

impl AppState {
    pub fn new(db: Db, config: Config, engine: EngineHandle) -> Self {
        Self {
            db,
            config: Arc::new(config),
            engine,
            login_limiter: Arc::new(LoginLimiter::default()),
            setup_lock: Arc::default(),
        }
    }
}
