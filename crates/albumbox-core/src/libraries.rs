//! Libraries: removable disks (or folders on them) identified by a marker file.

use serde::Serialize;

use crate::{CoreError, CoreResult, Db};

#[derive(Debug, Clone, Serialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub root_subpath: String,
    pub last_seen_root: Option<String>,
    pub is_online: bool,
    pub last_scan_at: Option<String>,
    pub created_at: String,
    pub file_count: i64,
    pub media_count: i64,
    pub missing_count: i64,
}

type Row = (String, String, String, Option<String>, bool, Option<String>, String, i64, i64, i64);

const SELECT: &str = "SELECT l.id, l.name, l.root_subpath, l.last_seen_root, l.is_online, l.last_scan_at, l.created_at,
        (SELECT COUNT(*) FROM media_files f WHERE f.library_id = l.id AND f.is_missing = 0),
        (SELECT COUNT(DISTINCT f.media_id) FROM media_files f WHERE f.library_id = l.id AND f.is_missing = 0),
        (SELECT COUNT(*) FROM media_files f WHERE f.library_id = l.id AND f.is_missing = 1)
    FROM libraries l";

fn from_row(r: Row) -> Library {
    Library {
        id: r.0,
        name: r.1,
        root_subpath: r.2,
        last_seen_root: r.3,
        is_online: r.4,
        last_scan_at: r.5,
        created_at: r.6,
        file_count: r.7,
        media_count: r.8,
        missing_count: r.9,
    }
}

pub async fn list(db: &Db) -> CoreResult<Vec<Library>> {
    let rows: Vec<Row> = sqlx::query_as(&format!("{SELECT} ORDER BY l.name COLLATE NOCASE")).fetch_all(&db.pool).await?;
    Ok(rows.into_iter().map(from_row).collect())
}

pub async fn get(db: &Db, id: &str) -> CoreResult<Option<Library>> {
    let row: Option<Row> = sqlx::query_as(&format!("{SELECT} WHERE l.id = ?")).bind(id).fetch_optional(&db.pool).await?;
    Ok(row.map(from_row))
}

pub async fn insert(db: &Db, id: &str, name: &str, root_subpath: &str, root: &str) -> CoreResult<Library> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::Invalid("name_required"));
    }
    sqlx::query(
        "INSERT INTO libraries (id, name, root_subpath, last_seen_root, is_online) VALUES (?, ?, ?, ?, 1)",
    )
    .bind(id)
    .bind(name)
    .bind(root_subpath)
    .bind(root)
    .execute(&db.pool)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => CoreError::Conflict("library_exists"),
        _ => CoreError::Db(e),
    })?;
    get(db, id).await?.ok_or(CoreError::NotFound)
}

pub async fn rename(db: &Db, id: &str, name: &str) -> CoreResult<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CoreError::Invalid("name_required"));
    }
    let r = sqlx::query("UPDATE libraries SET name = ? WHERE id = ?").bind(name).bind(id).execute(&db.pool).await?;
    if r.rows_affected() == 0 { Err(CoreError::NotFound) } else { Ok(()) }
}

/// Records where the library is mounted now. Returns true when anything changed.
pub async fn set_location(db: &Db, id: &str, root: Option<&str>) -> CoreResult<bool> {
    let r = sqlx::query(
        "UPDATE libraries
         SET is_online = ?, last_seen_root = COALESCE(?, last_seen_root)
         WHERE id = ? AND (is_online <> ? OR (? IS NOT NULL AND COALESCE(last_seen_root, '') <> ?))",
    )
    .bind(root.is_some())
    .bind(root)
    .bind(id)
    .bind(root.is_some())
    .bind(root)
    .bind(root)
    .execute(&db.pool)
    .await?;
    Ok(r.rows_affected() > 0)
}

pub async fn set_scanned_now(db: &Db, id: &str) -> CoreResult<()> {
    sqlx::query("UPDATE libraries SET last_scan_at = strftime('%Y-%m-%dT%H:%M:%SZ','now') WHERE id = ?")
        .bind(id)
        .execute(&db.pool)
        .await?;
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct FolderSummary {
    pub folder_rel: String,
    /// Event name suggested from the folder name ("Ramazan_Tatili" → "Ramazan Tatili").
    pub suggestion: String,
    pub media_count: i64,
    /// Up to four media ids with ready thumbnails, for previews.
    pub sample_media_ids: Vec<i64>,
    /// Event tags already attached to every media item in the folder.
    pub tags: Vec<String>,
}

/// Last folder segment, separators turned into spaces.
pub fn folder_suggestion(folder_rel: &str) -> String {
    let last = folder_rel.rsplit('\\').next().unwrap_or_default();
    last.replace(['_', '.'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Folders of a library that directly contain media, with previews.
pub async fn folders(db: &Db, library_id: &str) -> CoreResult<Vec<FolderSummary>> {
    let rows: Vec<(String, i64, Option<String>)> = sqlx::query_as(
        "SELECT f.folder_rel, COUNT(DISTINCT f.media_id),
                (SELECT group_concat(id) FROM (
                    SELECT DISTINCT m.id FROM media_files f2 JOIN media m ON m.id = f2.media_id
                    WHERE f2.library_id = f.library_id AND f2.folder_rel = f.folder_rel
                      AND f2.is_missing = 0 AND m.thumb_ready = 1
                    ORDER BY m.taken_at LIMIT 4))
         FROM media_files f
         WHERE f.library_id = ? AND f.is_missing = 0
         GROUP BY f.folder_rel
         ORDER BY f.folder_rel COLLATE NOCASE",
    )
    .bind(library_id)
    .fetch_all(&db.pool)
    .await?;

    let tag_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT f.folder_rel, t.name
         FROM media_files f
         JOIN media_tags mt ON mt.media_id = f.media_id
         JOIN tags t ON t.id = mt.tag_id
         JOIN tag_types tt ON tt.id = t.type_id AND tt.code = 'event'
         WHERE f.library_id = ? AND f.is_missing = 0
         GROUP BY f.folder_rel, t.id
         HAVING COUNT(DISTINCT f.media_id) = (
             SELECT COUNT(DISTINCT f3.media_id) FROM media_files f3
             WHERE f3.library_id = f.library_id AND f3.folder_rel = f.folder_rel AND f3.is_missing = 0)",
    )
    .bind(library_id)
    .fetch_all(&db.pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(folder_rel, media_count, samples)| FolderSummary {
            tags: tag_rows.iter().filter(|(f, _)| *f == folder_rel).map(|(_, t)| t.clone()).collect(),
            suggestion: folder_suggestion(&folder_rel),
            folder_rel,
            media_count,
            sample_media_ids: samples
                .unwrap_or_default()
                .split(',')
                .filter_map(|s| s.parse().ok())
                .collect(),
        })
        .collect())
}
