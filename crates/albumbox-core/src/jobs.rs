//! Persistent background job queue (scan, thumbnails, …).

use serde::Serialize;

use crate::{CoreResult, Db};

#[derive(Debug, Clone, Serialize)]
pub struct Job {
    pub id: i64,
    pub kind: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub progress_done: i64,
    pub progress_total: i64,
    pub message: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

type Row = (i64, String, String, String, i64, i64, Option<String>, String, Option<String>, Option<String>);

const COLUMNS: &str =
    "id, kind, payload, status, progress_done, progress_total, message, created_at, started_at, finished_at";

fn from_row(r: Row) -> Job {
    Job {
        id: r.0,
        kind: r.1,
        payload: serde_json::from_str(&r.2).unwrap_or(serde_json::Value::Null),
        status: r.3,
        progress_done: r.4,
        progress_total: r.5,
        message: r.6,
        created_at: r.7,
        started_at: r.8,
        finished_at: r.9,
    }
}

/// Queues a job unless an identical one is already queued or running; returns its id.
pub async fn enqueue(db: &Db, kind: &str, payload: &serde_json::Value) -> CoreResult<i64> {
    let payload = payload.to_string();
    let existing: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM jobs WHERE kind = ? AND payload = ? AND status IN ('queued', 'running') LIMIT 1",
    )
    .bind(kind)
    .bind(&payload)
    .fetch_optional(&db.pool)
    .await?;
    if let Some(id) = existing {
        return Ok(id);
    }
    Ok(sqlx::query_scalar("INSERT INTO jobs (kind, payload) VALUES (?, ?) RETURNING id")
        .bind(kind)
        .bind(payload)
        .fetch_one(&db.pool)
        .await?)
}

pub async fn next_queued(db: &Db) -> CoreResult<Option<Job>> {
    let row: Option<Row> =
        sqlx::query_as(&format!("SELECT {COLUMNS} FROM jobs WHERE status = 'queued' ORDER BY id LIMIT 1"))
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.map(from_row))
}

pub async fn count_queued(db: &Db) -> CoreResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE status = 'queued'").fetch_one(&db.pool).await?)
}

pub async fn mark_running(db: &Db, id: i64) -> CoreResult<()> {
    sqlx::query(
        "UPDATE jobs SET status = 'running', started_at = strftime('%Y-%m-%dT%H:%M:%SZ','now'), message = NULL WHERE id = ?",
    )
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

pub async fn set_progress(db: &Db, id: i64, done: i64, total: i64) -> CoreResult<()> {
    sqlx::query("UPDATE jobs SET progress_done = ?, progress_total = ? WHERE id = ?")
        .bind(done)
        .bind(total)
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

/// `status`: 'done' | 'failed' | 'cancelled' | 'queued' (re-queue after shutdown).
pub async fn finish(db: &Db, id: i64, status: &str, message: Option<&str>) -> CoreResult<()> {
    sqlx::query(
        "UPDATE jobs SET status = ?, message = ?,
            finished_at = CASE WHEN ? = 'queued' THEN NULL ELSE strftime('%Y-%m-%dT%H:%M:%SZ','now') END
         WHERE id = ?",
    )
    .bind(status)
    .bind(message)
    .bind(status)
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// Cancels a job that has not started yet. Returns true when it was queued.
pub async fn cancel_queued(db: &Db, id: i64) -> CoreResult<bool> {
    let r = sqlx::query(
        "UPDATE jobs SET status = 'cancelled', finished_at = strftime('%Y-%m-%dT%H:%M:%SZ','now')
         WHERE id = ? AND status = 'queued'",
    )
    .bind(id)
    .execute(&db.pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

/// Jobs left 'running' by a crash or restart go back to the queue.
pub async fn requeue_interrupted(db: &Db) -> CoreResult<u64> {
    let r = sqlx::query("UPDATE jobs SET status = 'queued' WHERE status = 'running'").execute(&db.pool).await?;
    Ok(r.rows_affected())
}

pub async fn recent(db: &Db, limit: i64) -> CoreResult<Vec<Job>> {
    let rows: Vec<Row> = sqlx::query_as(&format!("SELECT {COLUMNS} FROM jobs ORDER BY id DESC LIMIT ?"))
        .bind(limit)
        .fetch_all(&db.pool)
        .await?;
    Ok(rows.into_iter().map(from_row).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn enqueue_dedupes_and_requeues() {
        let db = Db::open_memory().await.unwrap();
        let a = enqueue(&db, "scan", &json!({ "library_id": "x" })).await.unwrap();
        assert_eq!(enqueue(&db, "scan", &json!({ "library_id": "x" })).await.unwrap(), a);
        let b = enqueue(&db, "scan", &json!({ "library_id": "y" })).await.unwrap();
        assert_ne!(a, b);

        mark_running(&db, a).await.unwrap();
        assert_eq!(next_queued(&db).await.unwrap().unwrap().id, b);
        assert_eq!(requeue_interrupted(&db).await.unwrap(), 1);
        assert!(cancel_queued(&db, b).await.unwrap());
        assert_eq!(next_queued(&db).await.unwrap().unwrap().id, a);
    }
}
