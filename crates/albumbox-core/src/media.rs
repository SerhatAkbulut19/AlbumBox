//! Media rows written by the scanner.

use std::collections::HashMap;

use serde::Serialize;

use crate::{CoreResult, Db};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    Photo,
    Video,
}

impl MediaKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MediaKind::Photo => "photo",
            MediaKind::Video => "video",
        }
    }

    pub fn parse(s: &str) -> MediaKind {
        if s == "video" { MediaKind::Video } else { MediaKind::Photo }
    }
}

/// Metadata extracted from one file.
#[derive(Debug, Clone, Default)]
pub struct ScannedMedia {
    pub hash: String,
    pub kind: Option<MediaKind>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    /// Local wall-clock time, "YYYY-MM-DDTHH:MM:SS".
    pub taken_at: Option<String>,
    /// 'exif' | 'video_meta' | 'filename' | 'mtime'
    pub taken_at_source: Option<&'static str>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub camera: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KnownFile {
    pub id: i64,
    pub size: i64,
    pub mtime: i64,
}

/// Files already indexed for a library, keyed by relative path.
pub async fn known_files(db: &Db, library_id: &str) -> CoreResult<HashMap<String, KnownFile>> {
    let rows: Vec<(i64, String, i64, i64)> =
        sqlx::query_as("SELECT id, rel_path, size, mtime FROM media_files WHERE library_id = ?")
            .bind(library_id)
            .fetch_all(&db.pool)
            .await?;
    Ok(rows.into_iter().map(|(id, rel, size, mtime)| (rel, KnownFile { id, size, mtime })).collect())
}

/// Marks unchanged files as seen in this scan. One short statement per 500
/// rows (each its own transaction) so API writes never wait behind a big scan.
pub async fn touch_files(db: &Db, ids: &[i64], seen_at: &str) -> CoreResult<()> {
    for chunk in ids.chunks(500) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!("UPDATE media_files SET last_seen_at = ?, is_missing = 0 WHERE id IN ({placeholders})");
        let mut q = sqlx::query(&sql).bind(seen_at);
        for id in chunk {
            q = q.bind(id);
        }
        q.execute(&db.pool).await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    /// First time this content is seen.
    NewMedia,
    /// Content already indexed (copy elsewhere, or a moved/renamed file).
    ExistingMedia,
}

/// Records a new or changed file and links it to its media row (by hash).
pub async fn upsert_file(
    db: &Db,
    library_id: &str,
    rel_path: &str,
    folder_rel: &str,
    size: i64,
    mtime: i64,
    seen_at: &str,
    m: &ScannedMedia,
) -> CoreResult<UpsertOutcome> {
    let mut tx = db.pool.begin().await?;
    let existing: Option<i64> =
        sqlx::query_scalar("SELECT id FROM media WHERE hash = ?").bind(&m.hash).fetch_optional(&mut *tx).await?;
    let (media_id, outcome) = match existing {
        Some(id) => (id, UpsertOutcome::ExistingMedia),
        None => {
            let id: i64 = sqlx::query_scalar(
                "INSERT INTO media (hash, kind, width, height, duration_ms, taken_at, taken_at_source, gps_lat, gps_lon, camera)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(&m.hash)
            .bind(m.kind.unwrap_or(MediaKind::Photo).as_str())
            .bind(m.width)
            .bind(m.height)
            .bind(m.duration_ms)
            .bind(&m.taken_at)
            .bind(m.taken_at_source)
            .bind(m.gps_lat)
            .bind(m.gps_lon)
            .bind(&m.camera)
            .fetch_one(&mut *tx)
            .await?;
            (id, UpsertOutcome::NewMedia)
        }
    };
    sqlx::query(
        "INSERT INTO media_files (media_id, library_id, rel_path, folder_rel, folder_norm, size, mtime, is_missing, first_seen_at, last_seen_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 0, ?, ?)
         ON CONFLICT (library_id, rel_path) DO UPDATE SET
            media_id = excluded.media_id, folder_rel = excluded.folder_rel, folder_norm = excluded.folder_norm,
            size = excluded.size, mtime = excluded.mtime, is_missing = 0, last_seen_at = excluded.last_seen_at",
    )
    .bind(media_id)
    .bind(library_id)
    .bind(rel_path)
    .bind(folder_rel)
    .bind(folder_norm(folder_rel))
    .bind(size)
    .bind(mtime)
    .bind(seen_at)
    .bind(seen_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(outcome)
}

/// Searchable form of a folder path: "Köy 2022\Nişan_Töreni" → "koy 2022 nisan toreni".
pub fn folder_norm(folder_rel: &str) -> String {
    crate::text::normalize(&folder_rel.replace(['\\', '/', '_', '-', '.'], " "))
}

/// Fills `folder_norm` for rows written before it existed.
pub async fn backfill_folder_norm(db: &Db) -> CoreResult<u64> {
    let rows: Vec<(i64, String)> =
        sqlx::query_as("SELECT id, folder_rel FROM media_files WHERE folder_norm IS NULL").fetch_all(&db.pool).await?;
    let mut tx = db.pool.begin().await?;
    for (id, folder) in &rows {
        sqlx::query("UPDATE media_files SET folder_norm = ? WHERE id = ?")
            .bind(folder_norm(folder))
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(rows.len() as u64)
}

/// Files not seen since `scan_started` are marked missing. Returns how many.
pub async fn mark_missing(db: &Db, library_id: &str, scan_started: &str) -> CoreResult<u64> {
    let r = sqlx::query(
        "UPDATE media_files SET is_missing = 1 WHERE library_id = ? AND last_seen_at < ? AND is_missing = 0",
    )
    .bind(library_id)
    .bind(scan_started)
    .execute(&db.pool)
    .await?;
    Ok(r.rows_affected())
}

/// A media item that still needs thumbnails and has a reachable file.
#[derive(Debug, Clone)]
pub struct ThumbTask {
    pub media_id: i64,
    pub hash: String,
    pub kind: MediaKind,
    pub duration_ms: Option<i64>,
    pub root: String,
    pub rel_path: String,
}

pub async fn pending_thumbs(db: &Db, limit: i64) -> CoreResult<Vec<ThumbTask>> {
    let rows: Vec<(i64, String, String, Option<i64>, String, String)> = sqlx::query_as(
        "SELECT m.id, m.hash, m.kind, m.duration_ms, l.last_seen_root, f.rel_path
         FROM media m
         JOIN media_files f ON f.id = (
             SELECT f2.id FROM media_files f2 JOIN libraries l2 ON l2.id = f2.library_id
             WHERE f2.media_id = m.id AND f2.is_missing = 0 AND l2.is_online = 1 LIMIT 1)
         JOIN libraries l ON l.id = f.library_id
         WHERE m.thumb_ready = 0
         ORDER BY m.id
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(media_id, hash, kind, duration_ms, root, rel_path)| ThumbTask {
            media_id,
            hash,
            kind: MediaKind::parse(&kind),
            duration_ms,
            root,
            rel_path,
        })
        .collect())
}

pub async fn count_pending_thumbs(db: &Db) -> CoreResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COUNT(*) FROM media m WHERE m.thumb_ready = 0 AND EXISTS (
            SELECT 1 FROM media_files f JOIN libraries l ON l.id = f.library_id
            WHERE f.media_id = m.id AND f.is_missing = 0 AND l.is_online = 1)",
    )
    .fetch_one(&db.pool)
    .await?)
}

/// `state`: 1 = ready, -1 = failed. Fills in dimensions when they were unknown.
pub async fn set_thumb_state(db: &Db, media_id: i64, state: i64, dims: Option<(i64, i64)>) -> CoreResult<()> {
    sqlx::query(
        "UPDATE media SET thumb_ready = ?, width = COALESCE(width, ?), height = COALESCE(height, ?) WHERE id = ?",
    )
    .bind(state)
    .bind(dims.map(|d| d.0))
    .bind(dims.map(|d| d.1))
    .bind(media_id)
    .execute(&db.pool)
    .await?;
    Ok(())
}

/// Reachable original file of a media item: (library root, relative path, kind).
pub async fn original_location(db: &Db, media_id: i64) -> CoreResult<Option<(String, String, MediaKind)>> {
    let row: Option<(String, String, String)> = sqlx::query_as(
        "SELECT l.last_seen_root, f.rel_path, m.kind
         FROM media_files f
         JOIN libraries l ON l.id = f.library_id
         JOIN media m ON m.id = f.media_id
         WHERE f.media_id = ? AND f.is_missing = 0 AND l.is_online = 1
         LIMIT 1",
    )
    .bind(media_id)
    .fetch_optional(&db.pool)
    .await?;
    Ok(row.map(|(root, rel, kind)| (root, rel, MediaKind::parse(&kind))))
}

pub async fn hash_of(db: &Db, media_id: i64) -> CoreResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT hash FROM media WHERE id = ?").bind(media_id).fetch_optional(&db.pool).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libraries;

    fn scanned(hash: &str) -> ScannedMedia {
        ScannedMedia { hash: hash.into(), kind: Some(MediaKind::Photo), ..Default::default() }
    }

    #[tokio::test]
    async fn duplicates_share_media_and_missing_files_are_flagged() {
        let db = Db::open_memory().await.unwrap();
        libraries::insert(&db, "lib1", "Aile Diski", "", "X:\\").await.unwrap();

        let t1 = "2026-01-01T00:00:00Z";
        assert_eq!(
            upsert_file(&db, "lib1", "Köy\\a.jpg", "Köy", 10, 1, t1, &scanned("h1")).await.unwrap(),
            UpsertOutcome::NewMedia
        );
        assert_eq!(
            upsert_file(&db, "lib1", "Yedek\\a.jpg", "Yedek", 10, 1, t1, &scanned("h1")).await.unwrap(),
            UpsertOutcome::ExistingMedia
        );
        let lib = libraries::get(&db, "lib1").await.unwrap().unwrap();
        assert_eq!((lib.file_count, lib.media_count), (2, 1));

        // Second scan only sees the first file.
        let t2 = "2026-01-02T00:00:00Z";
        let known = known_files(&db, "lib1").await.unwrap();
        touch_files(&db, &[known["Köy\\a.jpg"].id], t2).await.unwrap();
        assert_eq!(mark_missing(&db, "lib1", t2).await.unwrap(), 1);
        let lib = libraries::get(&db, "lib1").await.unwrap().unwrap();
        assert_eq!((lib.file_count, lib.missing_count), (1, 1));
        assert_eq!(pending_thumbs(&db, 10).await.unwrap().len(), 1);
    }
}
