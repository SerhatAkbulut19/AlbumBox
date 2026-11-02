//! Visibility: what a signed-in identity may see.
//!
//! Admins see everything. Members see a media item only when `media_access`
//! holds one of their profiles (see PLAN.md, "Görünürlük kuralı").

use serde::Serialize;

use crate::{
    users::{Role, User},
    CoreResult, Db,
};

/// SQL predicate restricting `<alias>.id` to media visible to member `?`.
/// Bind the user id once where it appears.
pub const MEMBER_MEDIA_FILTER: &str = "EXISTS (SELECT 1 FROM media_access ma
        JOIN user_profiles up ON up.profile_id = ma.profile_id
        WHERE ma.media_id = m.id AND up.user_id = ?)";

/// Rebuilds `media_access` for the given media (JSON array of ids passed as
/// one parameter, so any number of ids works):
/// - hidden media: nobody
/// - `custom`: exactly its `media_shares` profiles (none = private)
/// - `inherit`: union of the shares of every category it belongs to
pub async fn recompute(conn: &mut sqlx::SqliteConnection, media_ids: &[i64]) -> CoreResult<()> {
    if media_ids.is_empty() {
        return Ok(());
    }
    let ids = serde_json::to_string(media_ids).unwrap_or_else(|_| "[]".into());
    sqlx::query("DELETE FROM media_access WHERE media_id IN (SELECT value FROM json_each(?))")
        .bind(&ids)
        .execute(&mut *conn)
        .await?;
    sqlx::query(
        "INSERT OR IGNORE INTO media_access (profile_id, media_id)
         SELECT ms.profile_id, m.id
           FROM media m JOIN media_shares ms ON ms.media_id = m.id
          WHERE m.visibility_mode = 'custom' AND m.is_hidden = 0
            AND m.id IN (SELECT value FROM json_each(?1))
         UNION
         SELECT cs.profile_id, m.id
           FROM media m
           JOIN media_categories mc ON mc.media_id = m.id
           JOIN category_shares cs ON cs.category_id = mc.category_id
          WHERE m.visibility_mode = 'inherit' AND m.is_hidden = 0
            AND m.id IN (SELECT value FROM json_each(?1))",
    )
    .bind(&ids)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub async fn recompute_db(db: &Db, media_ids: &[i64]) -> CoreResult<()> {
    let mut tx = db.pool.begin().await?;
    recompute(&mut tx, media_ids).await?;
    tx.commit().await?;
    Ok(())
}

/// Media ids in the given categories (used before recomputing after share changes).
pub async fn media_in_categories(db: &Db, category_ids: &[i64]) -> CoreResult<Vec<i64>> {
    let ids = serde_json::to_string(category_ids).unwrap_or_else(|_| "[]".into());
    Ok(sqlx::query_scalar(
        "SELECT DISTINCT media_id FROM media_categories WHERE category_id IN (SELECT value FROM json_each(?))",
    )
    .bind(ids)
    .fetch_all(&db.pool)
    .await?)
}

/// Whether the user may see a media item (thumbnail, original, metadata).
pub async fn can_view(db: &Db, user: &User, media_id: i64) -> CoreResult<bool> {
    if user.role == Role::Admin {
        return Ok(true);
    }
    let visible: Option<i64> = sqlx::query_scalar(&format!(
        "SELECT 1 FROM media m WHERE m.id = ? AND m.is_hidden = 0 AND {MEMBER_MEDIA_FILTER}"
    ))
    .bind(media_id)
    .bind(user.id)
    .fetch_optional(&db.pool)
    .await?;
    Ok(visible.is_some())
}

#[derive(Debug, Serialize)]
pub struct HomeSummary {
    pub display_name: String,
    pub visible_media: i64,
    pub visible_categories: i64,
}

pub async fn home_summary(db: &Db, user: &User) -> CoreResult<HomeSummary> {
    let (visible_media, visible_categories): (i64, i64) = match user.role {
        Role::Admin => {
            sqlx::query_as(
                "SELECT (SELECT COUNT(*) FROM media WHERE is_hidden = 0), (SELECT COUNT(*) FROM categories)",
            )
            .fetch_one(&db.pool)
            .await?
        }
        Role::Member => {
            sqlx::query_as(&format!(
                "SELECT
                    (SELECT COUNT(*) FROM media m WHERE m.is_hidden = 0 AND {MEMBER_MEDIA_FILTER}),
                    (SELECT COUNT(DISTINCT c.id) FROM categories c
                       LEFT JOIN category_shares cs ON cs.category_id = c.id
                       LEFT JOIN user_profiles up1 ON up1.profile_id = cs.profile_id AND up1.user_id = ?
                       WHERE up1.user_id IS NOT NULL
                          OR EXISTS (SELECT 1 FROM media_categories mc
                                     JOIN media m ON m.id = mc.media_id AND m.is_hidden = 0
                                     WHERE mc.category_id = c.id AND {MEMBER_MEDIA_FILTER}))"
            ))
            .bind(user.id)
            .bind(user.id)
            .bind(user.id)
            .fetch_one(&db.pool)
            .await?
        }
    };
    Ok(HomeSummary { display_name: user.display_name.clone(), visible_media, visible_categories })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::{self, NewUser};

    async fn user(db: &Db, name: &str, role: Role) -> User {
        users::create(db, NewUser { username: name.into(), display_name: name.into(), password_hash: "x".into(), role })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn member_sees_only_shared_media() {
        let db = Db::open_memory().await.unwrap();
        let admin = user(&db, "admin", Role::Admin).await;
        let member = user(&db, "ahmet", Role::Member).await;
        for (id, hash) in [(1, "a"), (2, "b"), (3, "c")] {
            sqlx::query("INSERT INTO media (id, hash, kind) VALUES (?, ?, 'photo')")
                .bind(id)
                .bind(hash)
                .execute(&db.pool)
                .await
                .unwrap();
        }
        // Media 2 is published to "Tüm Aile".
        sqlx::query("INSERT INTO media_access (profile_id, media_id) VALUES (1, 2)").execute(&db.pool).await.unwrap();

        assert_eq!(home_summary(&db, &admin).await.unwrap().visible_media, 3);
        assert_eq!(home_summary(&db, &member).await.unwrap().visible_media, 1);
        assert!(can_view(&db, &member, 2).await.unwrap());
        assert!(!can_view(&db, &member, 1).await.unwrap(), "private media");
        assert!(can_view(&db, &admin, 1).await.unwrap());
    }
}
