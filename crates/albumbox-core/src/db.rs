use std::{path::Path, str::FromStr, time::Duration};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous};

use crate::CoreResult;

/// Shared SQLite pool. Cheap to clone.
#[derive(Clone)]
pub struct Db {
    pub pool: SqlitePool,
}

fn file_options(path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(10))
        .pragma("temp_store", "memory")
        .pragma("cache_size", "-65536") // 64 MB page cache per connection
}

impl Db {
    /// Opens (creating if needed) the database file and runs migrations.
    /// This pool serves the API.
    pub async fn open(path: &Path) -> CoreResult<Self> {
        let db = Self::connect(file_options(path), 8).await?;
        sqlx::migrate!("./migrations").run(&db.pool).await?;
        crate::media::backfill_folder_norm(&db).await?;
        Ok(db)
    }

    /// A second pool on the same (already migrated) file for the background
    /// engine, so scans and face jobs never take connections the API needs.
    /// WAL mode lets the API keep reading while the engine writes.
    pub async fn open_secondary(path: &Path, max: u32) -> CoreResult<Self> {
        Self::connect(file_options(path), max).await
    }

    /// In-memory database for tests.
    pub async fn open_memory() -> CoreResult<Self> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        // A single connection keeps the in-memory database alive and shared.
        let db = Self::connect(options, 1).await?;
        sqlx::migrate!("./migrations").run(&db.pool).await?;
        Ok(db)
    }

    async fn connect(options: SqliteConnectOptions, max: u32) -> CoreResult<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(max)
            .min_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(options)
            .await?;
        Ok(Self { pool })
    }
}
