use serde::Serialize;

use crate::{CoreResult, Db};

#[derive(Debug, Serialize)]
pub struct AdminStats {
    pub media: i64,
    pub photos: i64,
    pub videos: i64,
    pub published_media: i64,
    pub persons: i64,
    pub tags: i64,
    pub categories: i64,
    pub users: i64,
    pub profiles: i64,
    pub libraries: i64,
    pub libraries_online: i64,
    pub pending_face_suggestions: i64,
}

pub async fn admin_stats(db: &Db) -> CoreResult<AdminStats> {
    let row: (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM media),
            (SELECT COUNT(*) FROM media WHERE kind = 'photo'),
            (SELECT COUNT(*) FROM media WHERE kind = 'video'),
            (SELECT COUNT(DISTINCT media_id) FROM media_access),
            (SELECT COUNT(*) FROM persons),
            (SELECT COUNT(*) FROM tags),
            (SELECT COUNT(*) FROM categories),
            (SELECT COUNT(*) FROM users),
            (SELECT COUNT(*) FROM profiles),
            (SELECT COUNT(*) FROM libraries),
            (SELECT COUNT(*) FROM libraries WHERE is_online = 1),
            (SELECT COUNT(*) FROM faces WHERE status = 'suggested')",
    )
    .fetch_one(&db.pool)
    .await?;
    Ok(AdminStats {
        media: row.0,
        photos: row.1,
        videos: row.2,
        published_media: row.3,
        persons: row.4,
        tags: row.5,
        categories: row.6,
        users: row.7,
        profiles: row.8,
        libraries: row.9,
        libraries_online: row.10,
        pending_face_suggestions: row.11,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_database_stats() {
        let db = Db::open_memory().await.unwrap();
        let s = admin_stats(&db).await.unwrap();
        assert_eq!(s.media, 0);
        assert_eq!(s.profiles, 1, "built-in 'Tüm Aile' profile");
    }
}
