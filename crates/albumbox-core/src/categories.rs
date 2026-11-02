//! Categories: the album tree. A category's shares decide who sees the media
//! inside it (unless a media item has its own custom visibility).

use serde::Serialize;

use crate::{access, CoreError, CoreResult, Db};

#[derive(Debug, Clone, Serialize)]
pub struct Category {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    pub sort_order: i64,
    /// Explicit cover, or the earliest media item with a thumbnail.
    pub cover_media_id: Option<i64>,
    /// Media directly in this category.
    pub media_count: i64,
    /// Profiles the category is published to (empty = private).
    pub profile_ids: Vec<i64>,
}

pub async fn list(db: &Db) -> CoreResult<Vec<Category>> {
    let rows: Vec<(i64, Option<i64>, String, Option<String>, i64, Option<i64>, i64, Option<String>)> = sqlx::query_as(
        "SELECT c.id, c.parent_id, c.name, c.description, c.sort_order,
                COALESCE(c.cover_media_id, (
                    SELECT m.id FROM media_categories mc JOIN media m ON m.id = mc.media_id
                    WHERE mc.category_id = c.id AND m.thumb_ready = 1 AND m.is_hidden = 0
                    ORDER BY m.taken_at, m.id LIMIT 1)),
                (SELECT COUNT(*) FROM media_categories mc WHERE mc.category_id = c.id),
                (SELECT group_concat(profile_id) FROM category_shares cs WHERE cs.category_id = c.id)
         FROM categories c
         ORDER BY c.sort_order, c.name COLLATE NOCASE",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, parent_id, name, description, sort_order, cover_media_id, media_count, shares)| Category {
            id,
            parent_id,
            name,
            description,
            sort_order,
            cover_media_id,
            media_count,
            profile_ids: shares.unwrap_or_default().split(',').filter_map(|s| s.parse().ok()).collect(),
        })
        .collect())
}

fn clean_name(name: &str) -> CoreResult<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() { Err(CoreError::Invalid("name_required")) } else { Ok(name) }
}

pub async fn create(db: &Db, parent_id: Option<i64>, name: &str, description: Option<&str>) -> CoreResult<i64> {
    let name = clean_name(name)?;
    if let Some(p) = parent_id {
        exists(db, p).await?;
    }
    let next: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), 0) + 1 FROM categories WHERE parent_id IS ?")
        .bind(parent_id)
        .fetch_one(&db.pool)
        .await?;
    Ok(sqlx::query_scalar(
        "INSERT INTO categories (parent_id, name, description, sort_order) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(parent_id)
    .bind(name)
    .bind(description.map(str::trim).filter(|d| !d.is_empty()))
    .bind(next)
    .fetch_one(&db.pool)
    .await?)
}

async fn exists(db: &Db, id: i64) -> CoreResult<()> {
    let found: Option<i64> = sqlx::query_scalar("SELECT id FROM categories WHERE id = ?").bind(id).fetch_optional(&db.pool).await?;
    found.map(|_| ()).ok_or(CoreError::NotFound)
}

pub async fn update(db: &Db, id: i64, name: &str, description: Option<&str>) -> CoreResult<()> {
    let name = clean_name(name)?;
    let r = sqlx::query("UPDATE categories SET name = ?, description = ? WHERE id = ?")
        .bind(name)
        .bind(description.map(str::trim).filter(|d| !d.is_empty()))
        .bind(id)
        .execute(&db.pool)
        .await?;
    if r.rows_affected() == 0 { Err(CoreError::NotFound) } else { Ok(()) }
}

/// The category and all its descendants.
pub async fn subtree(db: &Db, id: i64) -> CoreResult<Vec<i64>> {
    Ok(sqlx::query_scalar(
        "WITH RECURSIVE t(id) AS (SELECT ? UNION ALL SELECT c.id FROM categories c JOIN t ON c.parent_id = t.id)
         SELECT id FROM t",
    )
    .bind(id)
    .fetch_all(&db.pool)
    .await?)
}

/// Moves a category under `parent_id` (None = top level) at position `sort_order`.
pub async fn move_to(db: &Db, id: i64, parent_id: Option<i64>, sort_order: Option<i64>) -> CoreResult<()> {
    exists(db, id).await?;
    if let Some(p) = parent_id {
        exists(db, p).await?;
        if subtree(db, id).await?.contains(&p) {
            return Err(CoreError::Invalid("category_cycle"));
        }
    }
    let mut tx = db.pool.begin().await?;
    let order = match sort_order {
        Some(o) => {
            // Make room at the target position.
            sqlx::query("UPDATE categories SET sort_order = sort_order + 1 WHERE parent_id IS ? AND sort_order >= ? AND id <> ?")
                .bind(parent_id)
                .bind(o)
                .bind(id)
                .execute(&mut *tx)
                .await?;
            o
        }
        None => {
            sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), 0) + 1 FROM categories WHERE parent_id IS ? AND id <> ?")
                .bind(parent_id)
                .bind(id)
                .fetch_one(&mut *tx)
                .await?
        }
    };
    sqlx::query("UPDATE categories SET parent_id = ?, sort_order = ? WHERE id = ?")
        .bind(parent_id)
        .bind(order)
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Deletes the category and its sub-categories. Media stay in the archive.
pub async fn delete(db: &Db, id: i64) -> CoreResult<()> {
    let ids = subtree(db, id).await?;
    let affected = access::media_in_categories(db, &ids).await?;
    let mut tx = db.pool.begin().await?;
    let r = sqlx::query("DELETE FROM categories WHERE id = ?").bind(id).execute(&mut *tx).await?;
    if r.rows_affected() == 0 {
        return Err(CoreError::NotFound);
    }
    access::recompute(&mut tx, &affected).await?;
    tx.commit().await?;
    Ok(())
}

/// Publishes the category to `profile_ids` (empty = private), optionally the
/// whole sub-tree too, and recomputes visibility of the media inside.
pub async fn set_shares(db: &Db, id: i64, profile_ids: &[i64], include_children: bool) -> CoreResult<()> {
    exists(db, id).await?;
    let targets = if include_children { subtree(db, id).await? } else { vec![id] };
    let affected = access::media_in_categories(db, &targets).await?;
    let mut tx = db.pool.begin().await?;
    for c in &targets {
        sqlx::query("DELETE FROM category_shares WHERE category_id = ?").bind(c).execute(&mut *tx).await?;
        for p in profile_ids {
            sqlx::query("INSERT OR IGNORE INTO category_shares (category_id, profile_id) VALUES (?, ?)")
                .bind(c)
                .bind(p)
                .execute(&mut *tx)
                .await?;
        }
    }
    access::recompute(&mut tx, &affected).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn set_cover(db: &Db, id: i64, media_id: Option<i64>) -> CoreResult<()> {
    let r = sqlx::query("UPDATE categories SET cover_media_id = ? WHERE id = ?")
        .bind(media_id)
        .bind(id)
        .execute(&db.pool)
        .await?;
    if r.rows_affected() == 0 { Err(CoreError::NotFound) } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn tree_moves_and_rejects_cycles() {
        let db = Db::open_memory().await.unwrap();
        let bayram = create(&db, None, "Bayramlar", None).await.unwrap();
        let ramazan = create(&db, Some(bayram), " Ramazan  2023 ", None).await.unwrap();
        let koy = create(&db, None, "Köy", None).await.unwrap();

        assert!(matches!(move_to(&db, bayram, Some(ramazan), None).await, Err(CoreError::Invalid("category_cycle"))));
        move_to(&db, ramazan, Some(koy), Some(1)).await.unwrap();
        let all = list(&db).await.unwrap();
        let r = all.iter().find(|c| c.id == ramazan).unwrap();
        assert_eq!((r.parent_id, r.name.as_str()), (Some(koy), "Ramazan 2023"));

        delete(&db, koy).await.unwrap();
        assert_eq!(list(&db).await.unwrap().len(), 1, "sub-tree deleted with parent");
    }
}
