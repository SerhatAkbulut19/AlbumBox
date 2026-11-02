//! Faces found in media, unnamed groups (clusters) and persons.
//!
//! Status of a face:
//! - `unassigned`: unnamed; `cluster_id` groups similar unnamed faces
//! - `suggested`: probably `person_tag_id`, waiting for confirmation
//! - `confirmed`: named; the media carries the person tag (source 'face')
//! - `rejected`: ignored (not a face, a stranger, or "not this person")

use serde::Serialize;

use crate::{CoreError, CoreResult, Db};

pub struct NewFace {
    /// x, y, width, height relative to the image.
    pub bbox: [f32; 4],
    pub score: f32,
    pub embedding: Vec<u8>,
}

/// Media with a thumbnail but not analysed yet.
pub async fn pending_media(db: &Db, limit: i64) -> CoreResult<Vec<(i64, String)>> {
    Ok(sqlx::query_as(
        "SELECT id, hash FROM media WHERE faces_done = 0 AND thumb_ready = 1 AND is_hidden = 0 ORDER BY id LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&db.pool)
    .await?)
}

pub async fn count_pending(db: &Db) -> CoreResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM media WHERE faces_done = 0 AND thumb_ready = 1 AND is_hidden = 0")
        .fetch_one(&db.pool)
        .await?)
}

/// Stores the faces of one media item (replacing earlier results) and marks it done.
pub async fn save_media_faces(db: &Db, media_id: i64, faces: &[NewFace]) -> CoreResult<Vec<i64>> {
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM faces WHERE media_id = ? AND status <> 'confirmed'").bind(media_id).execute(&mut *tx).await?;
    let mut ids = Vec::with_capacity(faces.len());
    for f in faces {
        ids.push(
            sqlx::query_scalar(
                "INSERT INTO faces (media_id, bbox_x, bbox_y, bbox_w, bbox_h, score, embedding) VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING id",
            )
            .bind(media_id)
            .bind(f.bbox[0])
            .bind(f.bbox[1])
            .bind(f.bbox[2])
            .bind(f.bbox[3])
            .bind(f.score)
            .bind(&f.embedding)
            .fetch_one(&mut *tx)
            .await?,
        );
    }
    sqlx::query("UPDATE media SET faces_done = 1 WHERE id = ?").bind(media_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(ids)
}

pub async fn mark_failed(db: &Db, media_id: i64) -> CoreResult<()> {
    sqlx::query("UPDATE media SET faces_done = -1 WHERE id = ?").bind(media_id).execute(&db.pool).await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Grouping input / output
// ---------------------------------------------------------------------------

/// Faces still open for grouping or suggestions (id, embedding).
pub async fn open_faces(db: &Db) -> CoreResult<Vec<(i64, Vec<u8>)>> {
    Ok(sqlx::query_as(
        "SELECT f.id, f.embedding FROM faces f JOIN media m ON m.id = f.media_id
         WHERE m.is_hidden = 0
           AND (f.status IN ('unassigned', 'suggested') OR (f.status = 'confirmed' AND f.person_tag_id IS NULL))
         ORDER BY f.id",
    )
    .fetch_all(&db.pool)
    .await?)
}

/// Confirmed faces per person: (person tag id, face id, score, embedding).
pub async fn confirmed_faces(db: &Db) -> CoreResult<Vec<(i64, i64, f64, Vec<u8>)>> {
    Ok(sqlx::query_as(
        "SELECT person_tag_id, id, score, embedding FROM faces
         WHERE status = 'confirmed' AND person_tag_id IS NOT NULL ORDER BY person_tag_id, id",
    )
    .fetch_all(&db.pool)
    .await?)
}

pub struct Assignment {
    pub face_id: i64,
    /// Suggested person, or `None` to leave unnamed.
    pub suggested: Option<i64>,
    pub cluster: Option<i64>,
}

/// Rows written per transaction: SQLite has a single writer, so short
/// transactions keep API writes from waiting behind a big regrouping.
const WRITE_CHUNK: usize = 500;

/// Writes suggestions and clusters for open faces, and the person summaries.
/// Only faces still open are touched, so a face confirmed meanwhile is kept.
pub async fn apply_grouping(
    db: &Db,
    assignments: &[Assignment],
    persons: &[(i64, Vec<u8>, i64, Option<i64>)],
) -> CoreResult<()> {
    const OPEN: &str = "(status IN ('unassigned', 'suggested') OR (status = 'confirmed' AND person_tag_id IS NULL))";
    let suggest_sql =
        format!("UPDATE faces SET status = 'suggested', person_tag_id = ?, cluster_id = NULL WHERE id = ? AND {OPEN}");
    let group_sql =
        format!("UPDATE faces SET status = 'unassigned', person_tag_id = NULL, cluster_id = ? WHERE id = ? AND {OPEN}");
    for chunk in assignments.chunks(WRITE_CHUNK) {
        let mut tx = db.pool.begin().await?;
        for a in chunk {
            match a.suggested {
                Some(p) => sqlx::query(&suggest_sql).bind(p).bind(a.face_id),
                None => sqlx::query(&group_sql).bind(a.cluster).bind(a.face_id),
            }
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
    }
    let mut tx = db.pool.begin().await?;
    sqlx::query("DELETE FROM persons").execute(&mut *tx).await?;
    for (tag_id, centroid, count, cover) in persons {
        sqlx::query("INSERT INTO persons (tag_id, centroid, face_count, cover_face_id) VALUES (?, ?, ?, ?)")
            .bind(tag_id)
            .bind(centroid)
            .bind(count)
            .bind(cover)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Admin views
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct FaceRef {
    pub id: i64,
    pub media_id: i64,
    pub score: f64,
}

#[derive(Debug, Serialize)]
pub struct Cluster {
    pub cluster_id: i64,
    pub count: i64,
    pub media_count: i64,
    pub faces: Vec<FaceRef>,
}

const SAMPLE: i64 = 12;

/// Unnamed groups, biggest first.
pub async fn clusters(db: &Db, min_size: i64) -> CoreResult<Vec<Cluster>> {
    let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
        "SELECT cluster_id, COUNT(*), COUNT(DISTINCT media_id) FROM faces
         WHERE status = 'unassigned' AND cluster_id IS NOT NULL
         GROUP BY cluster_id HAVING COUNT(*) >= ? ORDER BY COUNT(*) DESC, cluster_id LIMIT 200",
    )
    .bind(min_size)
    .fetch_all(&db.pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for (cluster_id, count, media_count) in rows {
        let faces = sqlx::query_as(
            "SELECT id, media_id, score FROM faces WHERE status = 'unassigned' AND cluster_id = ? ORDER BY score DESC LIMIT ?",
        )
        .bind(cluster_id)
        .bind(SAMPLE)
        .fetch_all(&db.pool)
        .await?;
        out.push(Cluster { cluster_id, count, media_count, faces });
    }
    Ok(out)
}

#[derive(Debug, Serialize)]
pub struct PersonSummary {
    pub tag_id: i64,
    pub name: String,
    pub confirmed: i64,
    pub suggested: i64,
    pub cover_face_id: Option<i64>,
}

/// Person tags that have faces (confirmed or suggested).
pub async fn persons(db: &Db) -> CoreResult<Vec<PersonSummary>> {
    let rows: Vec<(i64, String, i64, i64, Option<i64>)> = sqlx::query_as(
        "SELECT t.id, t.name,
                (SELECT COUNT(*) FROM faces f WHERE f.person_tag_id = t.id AND f.status = 'confirmed'),
                (SELECT COUNT(*) FROM faces f WHERE f.person_tag_id = t.id AND f.status = 'suggested'),
                COALESCE((SELECT cover_face_id FROM persons p WHERE p.tag_id = t.id),
                         (SELECT f.id FROM faces f WHERE f.person_tag_id = t.id ORDER BY f.score DESC LIMIT 1))
         FROM tags t JOIN tag_types tt ON tt.id = t.type_id AND tt.code = 'person'
         WHERE EXISTS (SELECT 1 FROM faces f WHERE f.person_tag_id = t.id AND f.status IN ('confirmed', 'suggested'))
         ORDER BY t.name COLLATE NOCASE",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(tag_id, name, confirmed, suggested, cover_face_id)| PersonSummary { tag_id, name, confirmed, suggested, cover_face_id })
        .collect())
}

pub async fn person_faces(db: &Db, tag_id: i64, status: &str, limit: i64) -> CoreResult<Vec<FaceRef>> {
    Ok(sqlx::query_as(
        "SELECT id, media_id, score FROM faces WHERE person_tag_id = ? AND status = ? ORDER BY score DESC, id LIMIT ?",
    )
    .bind(tag_id)
    .bind(status)
    .bind(limit)
    .fetch_all(&db.pool)
    .await?)
}

async fn ensure_person_tag(conn: &mut sqlx::SqliteConnection, tag_id: i64) -> CoreResult<()> {
    let ok: Option<i64> = sqlx::query_scalar(
        "SELECT t.id FROM tags t JOIN tag_types tt ON tt.id = t.type_id AND tt.code = 'person' WHERE t.id = ?",
    )
    .bind(tag_id)
    .fetch_optional(&mut *conn)
    .await?;
    ok.map(|_| ()).ok_or(CoreError::Invalid("not_a_person_tag"))
}

/// Names faces: confirmed as the person, and the person tag is added to their media.
pub async fn confirm(db: &Db, face_ids: &[i64], tag_id: i64) -> CoreResult<u64> {
    let ids = serde_json::to_string(face_ids).unwrap_or_default();
    let mut tx = db.pool.begin().await?;
    ensure_person_tag(&mut tx, tag_id).await?;
    let changed = sqlx::query(
        "UPDATE faces SET status = 'confirmed', person_tag_id = ?, cluster_id = NULL WHERE id IN (SELECT value FROM json_each(?))",
    )
    .bind(tag_id)
    .bind(&ids)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    sqlx::query(
        "INSERT OR IGNORE INTO media_tags (media_id, tag_id, source)
         SELECT DISTINCT media_id, ?, 'face' FROM faces WHERE id IN (SELECT value FROM json_each(?))",
    )
    .bind(tag_id)
    .bind(&ids)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(changed)
}

pub async fn cluster_face_ids(db: &Db, cluster_id: i64) -> CoreResult<Vec<i64>> {
    Ok(sqlx::query_scalar("SELECT id FROM faces WHERE status = 'unassigned' AND cluster_id = ?")
        .bind(cluster_id)
        .fetch_all(&db.pool)
        .await?)
}

/// Ignores faces ("not this person" / "not someone we know"). A person tag
/// added by face recognition is removed from media that no longer have a
/// confirmed face of that person.
pub async fn reject(db: &Db, face_ids: &[i64]) -> CoreResult<u64> {
    let ids = serde_json::to_string(face_ids).unwrap_or_default();
    let mut tx = db.pool.begin().await?;
    let affected: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT media_id, person_tag_id FROM faces
         WHERE id IN (SELECT value FROM json_each(?)) AND status = 'confirmed' AND person_tag_id IS NOT NULL",
    )
    .bind(&ids)
    .fetch_all(&mut *tx)
    .await?;
    let changed = sqlx::query(
        "UPDATE faces SET status = 'rejected', person_tag_id = NULL, cluster_id = NULL WHERE id IN (SELECT value FROM json_each(?))",
    )
    .bind(&ids)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    for (media_id, tag_id) in affected {
        sqlx::query(
            "DELETE FROM media_tags WHERE media_id = ? AND tag_id = ? AND source = 'face'
               AND NOT EXISTS (SELECT 1 FROM faces WHERE media_id = ? AND person_tag_id = ? AND status = 'confirmed')",
        )
        .bind(media_id)
        .bind(tag_id)
        .bind(media_id)
        .bind(tag_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(changed)
}

#[derive(Debug, Serialize)]
pub struct MediaFace {
    pub id: i64,
    pub bbox: [f64; 4],
    pub status: String,
    pub person_tag_id: Option<i64>,
    pub person_name: Option<String>,
}

/// Faces on one media item (for the admin viewer overlay).
pub async fn media_faces(db: &Db, media_id: i64) -> CoreResult<Vec<MediaFace>> {
    let rows: Vec<(i64, f64, f64, f64, f64, String, Option<i64>, Option<String>)> = sqlx::query_as(
        "SELECT f.id, f.bbox_x, f.bbox_y, f.bbox_w, f.bbox_h, f.status, f.person_tag_id, t.name
         FROM faces f LEFT JOIN tags t ON t.id = f.person_tag_id
         WHERE f.media_id = ? AND f.status <> 'rejected' ORDER BY f.bbox_x",
    )
    .bind(media_id)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, x, y, w, h, status, person_tag_id, person_name)| MediaFace { id, bbox: [x, y, w, h], status, person_tag_id, person_name })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct FaceStats {
    pub pending_media: i64,
    pub analysed_media: i64,
    pub faces: i64,
    pub unassigned: i64,
    pub suggested: i64,
    pub confirmed: i64,
}

pub async fn stats(db: &Db) -> CoreResult<FaceStats> {
    let (pending_media, analysed_media, faces, unassigned, suggested, confirmed): (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM media WHERE faces_done = 0 AND thumb_ready = 1 AND is_hidden = 0),
            (SELECT COUNT(*) FROM media WHERE faces_done = 1),
            (SELECT COUNT(*) FROM faces WHERE status <> 'rejected'),
            (SELECT COUNT(*) FROM faces WHERE status = 'unassigned'),
            (SELECT COUNT(*) FROM faces WHERE status = 'suggested'),
            (SELECT COUNT(*) FROM faces WHERE status = 'confirmed')",
    )
    .fetch_one(&db.pool)
    .await?;
    Ok(FaceStats { pending_media, analysed_media, faces, unassigned, suggested, confirmed })
}

/// Media id of a face (for crops and access checks).
pub async fn face_media(db: &Db, face_id: i64) -> CoreResult<Option<i64>> {
    Ok(sqlx::query_scalar("SELECT media_id FROM faces WHERE id = ?").bind(face_id).fetch_optional(&db.pool).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tags;

    #[tokio::test]
    async fn confirm_and_reject_keep_media_tags_in_sync() {
        let db = Db::open_memory().await.unwrap();
        sqlx::query("INSERT INTO media (id, hash, kind, thumb_ready) VALUES (1, 'a', 'photo', 1), (2, 'b', 'photo', 1)")
            .execute(&db.pool)
            .await
            .unwrap();
        assert_eq!(count_pending(&db).await.unwrap(), 2);
        let face = |x: f32| NewFace { bbox: [x, 0.1, 0.2, 0.2], score: 0.9, embedding: vec![0; 8] };
        let a = save_media_faces(&db, 1, &[face(0.1), face(0.5)]).await.unwrap();
        let b = save_media_faces(&db, 2, &[face(0.3)]).await.unwrap();
        assert_eq!(count_pending(&db).await.unwrap(), 0);

        let ahmet = tags::get_or_create(&db, "person", "Ahmet").await.unwrap();
        let place = tags::get_or_create(&db, "place", "Köy").await.unwrap();
        assert!(matches!(confirm(&db, &a, place).await, Err(CoreError::Invalid("not_a_person_tag"))));

        confirm(&db, &[a[0], b[0]], ahmet).await.unwrap();
        let tagged: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM media_tags WHERE tag_id = ?").bind(ahmet).fetch_one(&db.pool).await.unwrap();
        assert_eq!(tagged, 2);
        assert_eq!(persons(&db).await.unwrap()[0].confirmed, 2);

        // Wrong on media 2: tag removed there, kept on media 1.
        reject(&db, &[b[0]]).await.unwrap();
        let left: Vec<i64> = sqlx::query_scalar("SELECT media_id FROM media_tags WHERE tag_id = ?").bind(ahmet).fetch_all(&db.pool).await.unwrap();
        assert_eq!(left, vec![1]);
        assert_eq!(media_faces(&db, 1).await.unwrap()[0].person_name.as_deref(), Some("Ahmet"));
        assert_eq!(stats(&db).await.unwrap().confirmed, 1);
    }
}
