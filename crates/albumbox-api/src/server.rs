use std::{future::Future, net::SocketAddr, time::Duration};

use albumbox_core::{auth, Db};
use albumbox_indexer::EngineHandle;
use anyhow::Context;

use crate::{config::Config, routes, state::AppState};

/// Runs the HTTP server and the background engine until `shutdown` completes.
pub async fn run(config: Config, shutdown: impl Future<Output = ()> + Send + 'static) -> anyhow::Result<()> {
    std::fs::create_dir_all(&config.data_dir)
        .with_context(|| format!("create data dir {}", config.data_dir.display()))?;
    let db = Db::open(&config.db_path()).await.context("open database")?;
    tracing::info!(db = %config.db_path().display(), "database ready");

    spawn_session_cleanup(db.clone());
    // The engine gets its own connections: a long scan can never make the API wait.
    let engine_db = Db::open_secondary(&config.db_path(), 4).await.context("open engine database")?;
    let engine = EngineHandle::start(engine_db.clone(), config.engine());

    let bind = config.bind.clone();
    let app = routes::router(AppState::new(db.clone(), config, engine.clone()));
    let listener = tokio::net::TcpListener::bind(&bind).await.with_context(|| format!("bind {bind}"))?;
    tracing::info!(%bind, "AlbumBox API listening");

    let served = axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(shutdown)
        .await;

    // Let a running scan stop at a safe point; it resumes on next start.
    tokio::task::spawn_blocking(move || engine.shutdown()).await?;
    engine_db.pool.close().await;
    db.pool.close().await;
    tracing::info!("AlbumBox API stopped");
    served.map_err(Into::into)
}

fn spawn_session_cleanup(db: Db) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60 * 60));
        loop {
            tick.tick().await;
            match auth::purge_expired_sessions(&db).await {
                Ok(n) if n > 0 => tracing::info!(removed = n, "expired sessions purged"),
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "session purge failed"),
            }
        }
    });
}
