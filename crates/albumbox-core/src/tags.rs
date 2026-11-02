//! Tags and tag types (Kişi, Yer, Etkinlik, Tarih, Serbest + user-defined).

use serde::Serialize;

use crate::{text::normalize, CoreError, CoreResult, Db};

// ---------------------------------------------------------------------------
// Tag types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct TagType {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub color: Option<String>,
    pub is_builtin: bool,
    pub sort_order: i64,
    pub tag_count: i64,
}

pub async fn list_types(db: &Db) -> CoreResult<Vec<TagType>> {
    let rows: Vec<(i64, String, String, Option<String>, bool, i64, i64)> = sqlx::query_as(
        "SELECT tt.id, tt.code, tt.name, tt.color, tt.is_builtin, tt.sort_order,
                (SELECT COUNT(*) FROM tags t WHERE t.type_id = tt.id)
         FROM tag_types tt ORDER BY tt.sort_order, tt.id",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, code, name, color, is_builtin, sort_order, tag_count)| TagType {
            id,
            code,
            name,
            color,
            is_builtin,
            sort_order,
            tag_count,
        })
        .collect())
}

fn clean(name: &str) -> CoreResult<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() { Err(CoreError::Invalid("name_required")) } else { Ok(name) }
}

fn valid_color(color: Option<&str>) -> CoreResult<Option<String>> {
    match color.map(str::trim).filter(|c| !c.is_empty()) {
        None => Ok(None),
        Some(c) if c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|h| h.is_ascii_hexdigit()) => {
            Ok(Some(c.to_lowercase()))
        }
        Some(_) => Err(CoreError::Invalid("invalid_color")),
    }
}

pub async fn create_type(db: &Db, name: &str, color: Option<&str>) -> CoreResult<i64> {
    let name = clean(name)?;
    let code = normalize(&name).replace(' ', "_");
    let order: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), 0) + 1 FROM tag_types").fetch_one(&db.pool).await?;
    sqlx::query_scalar("INSERT INTO tag_types (code, name, color, sort_order) VALUES (?, ?, ?, ?) RETURNING id")
        .bind(code)
        .bind(name)
        .bind(valid_color(color)?)
        .bind(order)
        .fetch_one(&db.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.is_unique_violation() => CoreError::Conflict("tag_type_exists"),
            _ => CoreError::Db(e),
        })
}

pub async fn update_type(db: &Db, id: i64, name: &str, color: Option<&str>) -> CoreResult<()> {
    let r = sqlx::query("UPDATE tag_types SET name = ?, color = ? WHERE id = ?")
        .bind(clean(name)?)
        .bind(valid_color(color)?)
        .bind(id)
        .execute(&db.pool)
        .await?;
    if r.rows_affected() == 0 { Err(CoreError::NotFound) } else { Ok(()) }
}

/// Only empty, user-defined types can be deleted.
pub async fn delete_type(db: &Db, id: i64) -> CoreResult<()> {
    let row: Option<(bool, i64)> =
        sqlx::query_as("SELECT is_builtin, (SELECT COUNT(*) FROM tags WHERE type_id = tag_types.id) FROM tag_types WHERE id = ?")
            .bind(id)
            .fetch_optional(&db.pool)
            .await?;
    match row {
        None => Err(CoreError::NotFound),
        Some((true, _)) => Err(CoreError::Invalid("builtin_tag_type")),
        Some((_, n)) if n > 0 => Err(CoreError::Invalid("tag_type_not_empty")),
        Some(_) => {
            sqlx::query("DELETE FROM tag_types WHERE id = ?").bind(id).execute(&db.pool).await?;
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Tags
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Tag {
    pub id: i64,
    pub type_id: i64,
    pub type_code: String,
    pub name: String,
    pub parent_id: Option<i64>,
    pub parent_name: Option<String>,
    pub media_count: i64,
}

type TagRow = (i64, i64, String, String, Option<i64>, Option<String>, i64);

const TAG_SELECT: &str = "SELECT t.id, t.type_id, tt.code, t.name, t.parent_id, p.name,
        (SELECT COUNT(*) FROM media_tags mt WHERE mt.tag_id = t.id)
    FROM tags t
    JOIN tag_types tt ON tt.id = t.type_id
    LEFT JOIN tags p ON p.id = t.parent_id";

fn tag_from(r: TagRow) -> Tag {
    Tag { id: r.0, type_id: r.1, type_code: r.2, name: r.3, parent_id: r.4, parent_name: r.5, media_count: r.6 }
}

/// FTS5 prefix query for every word: "köy nişan" → `"koy"* "nisan"*`.
pub fn fts_query(q: &str) -> Option<String> {
    let parts: Vec<String> = normalize(q)
        .split(' ')
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).collect::<String>())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

pub async fn list(db: &Db, type_id: Option<i64>) -> CoreResult<Vec<Tag>> {
    let rows: Vec<TagRow> = sqlx::query_as(&format!(
        "{TAG_SELECT} WHERE (?1 IS NULL OR t.type_id = ?1) ORDER BY tt.sort_order, t.name COLLATE NOCASE"
    ))
    .bind(type_id)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows.into_iter().map(tag_from).collect())
}

/// Autocomplete: tags whose words start with the typed words, most used first.
pub async fn suggest(db: &Db, q: &str, type_id: Option<i64>, limit: i64) -> CoreResult<Vec<Tag>> {
    let Some(fts) = fts_query(q) else { return Ok(vec![]) };
    let rows: Vec<TagRow> = sqlx::query_as(&format!(
        "{TAG_SELECT}
         WHERE t.id IN (SELECT rowid FROM tags_fts WHERE tags_fts MATCH ?1)
           AND (?2 IS NULL OR t.type_id = ?2)
         ORDER BY (SELECT COUNT(*) FROM media_tags mt WHERE mt.tag_id = t.id) DESC, t.name COLLATE NOCASE
         LIMIT ?3"
    ))
    .bind(fts)
    .bind(type_id)
    .bind(limit)
    .fetch_all(&db.pool)
    .await?;
    Ok(rows.into_iter().map(tag_from).collect())
}

pub async fn get(db: &Db, id: i64) -> CoreResult<Option<Tag>> {
    let row: Option<TagRow> =
        sqlx::query_as(&format!("{TAG_SELECT} WHERE t.id = ?")).bind(id).fetch_optional(&db.pool).await?;
    Ok(row.map(tag_from))
}

/// Finds a tag by normalized name within its type, or creates it.
pub async fn get_or_create_in(db: &Db, type_id: i64, name: &str) -> CoreResult<i64> {
    let name = clean(name)?;
    let norm = normalize(&name);
    sqlx::query("INSERT INTO tags (type_id, name, name_norm) VALUES (?, ?, ?) ON CONFLICT (type_id, name_norm) DO NOTHING")
        .bind(type_id)
        .bind(&name)
        .bind(&norm)
        .execute(&db.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.is_foreign_key_violation() => CoreError::Invalid("unknown_tag_type"),
            _ => CoreError::Db(e),
        })?;
    Ok(sqlx::query_scalar("SELECT id FROM tags WHERE type_id = ? AND name_norm = ?")
        .bind(type_id)
        .bind(&norm)
        .fetch_one(&db.pool)
        .await?)
}

pub async fn rename(db: &Db, id: i64, name: &str) -> CoreResult<()> {
    let name = clean(name)?;
    let r = sqlx::query("UPDATE tags SET name = ?, name_norm = ? WHERE id = ?")
        .bind(&name)
        .bind(normalize(&name))
        .bind(id)
        .execute(&db.pool)
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(d) if d.is_unique_violation() => CoreError::Conflict("tag_exists"),
            _ => CoreError::Db(e),
        })?;
    if r.rows_affected() == 0 { Err(CoreError::NotFound) } else { Ok(()) }
}

/// Sets the parent tag (e.g. Köy → Sivas). Rejects cycles.
pub async fn set_parent(db: &Db, id: i64, parent_id: Option<i64>) -> CoreResult<()> {
    if let Some(p) = parent_id {
        let ancestors: Vec<i64> = sqlx::query_scalar(
            "WITH RECURSIVE a(id) AS (SELECT ? UNION ALL SELECT t.parent_id FROM tags t JOIN a ON t.id = a.id WHERE t.parent_id IS NOT NULL)
             SELECT id FROM a",
        )
        .bind(p)
        .fetch_all(&db.pool)
        .await?;
        if ancestors.contains(&id) {
            return Err(CoreError::Invalid("tag_cycle"));
        }
    }
    let r = sqlx::query("UPDATE tags SET parent_id = ? WHERE id = ?").bind(parent_id).bind(id).execute(&db.pool).await?;
    if r.rows_affected() == 0 { Err(CoreError::NotFound) } else { Ok(()) }
}

/// Moves every use of `source` to `target` and deletes `source`.
pub async fn merge(db: &Db, source: i64, target: i64) -> CoreResult<()> {
    if source == target {
        return Err(CoreError::Invalid("tag_merge_self"));
    }
    get(db, target).await?.ok_or(CoreError::NotFound)?;
    let mut tx = db.pool.begin().await?;
    sqlx::query("INSERT OR IGNORE INTO media_tags (media_id, tag_id, source, created_at) SELECT media_id, ?, source, created_at FROM media_tags WHERE tag_id = ?")
        .bind(target)
        .bind(source)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE faces SET person_tag_id = ? WHERE person_tag_id = ?").bind(target).bind(source).execute(&mut *tx).await?;
    sqlx::query("UPDATE tags SET parent_id = ? WHERE parent_id = ? AND id <> ?").bind(target).bind(source).bind(target).execute(&mut *tx).await?;
    let r = sqlx::query("DELETE FROM tags WHERE id = ?").bind(source).execute(&mut *tx).await?;
    if r.rows_affected() == 0 {
        return Err(CoreError::NotFound);
    }
    tx.commit().await?;
    Ok(())
}

pub async fn delete(db: &Db, id: i64) -> CoreResult<()> {
    let r = sqlx::query("DELETE FROM tags WHERE id = ?").bind(id).execute(&db.pool).await?;
    if r.rows_affected() == 0 { Err(CoreError::NotFound) } else { Ok(()) }
}

/// Finds a tag of the given type by normalized name, or creates it.
pub async fn get_or_create(db: &Db, type_code: &str, name: &str) -> CoreResult<i64> {
    let name = clean(name)?;
    let type_id: i64 = sqlx::query_scalar("SELECT id FROM tag_types WHERE code = ?")
        .bind(type_code)
        .fetch_optional(&db.pool)
        .await?
        .ok_or(CoreError::Invalid("unknown_tag_type"))?;
    let norm = normalize(&name);
    sqlx::query("INSERT INTO tags (type_id, name, name_norm) VALUES (?, ?, ?) ON CONFLICT (type_id, name_norm) DO NOTHING")
        .bind(type_id)
        .bind(&name)
        .bind(&norm)
        .execute(&db.pool)
        .await?;
    Ok(sqlx::query_scalar("SELECT id FROM tags WHERE type_id = ? AND name_norm = ?")
        .bind(type_id)
        .bind(&norm)
        .fetch_one(&db.pool)
        .await?)
}

/// Attaches a tag to every media item in a library folder (optionally with
/// subfolders). Returns the number of newly tagged media items.
pub async fn tag_folder(
    db: &Db,
    tag_id: i64,
    library_id: &str,
    folder_rel: &str,
    include_subfolders: bool,
) -> CoreResult<u64> {
    let prefix = if folder_rel.is_empty() { String::new() } else { format!("{folder_rel}\\") };
    let r = sqlx::query(
        "INSERT OR IGNORE INTO media_tags (media_id, tag_id, source)
         SELECT DISTINCT f.media_id, ?, 'folder_suggestion'
         FROM media_files f
         WHERE f.library_id = ? AND f.is_missing = 0
           AND (f.folder_rel = ? OR (? AND (? = '' OR substr(f.folder_rel, 1, length(?)) = ?)))",
    )
    .bind(tag_id)
    .bind(library_id)
    .bind(folder_rel)
    .bind(include_subfolders)
    .bind(&prefix)
    .bind(&prefix)
    .bind(&prefix)
    .execute(&db.pool)
    .await?;
    Ok(r.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{libraries, media};

    #[tokio::test]
    async fn folder_tagging_with_subfolders() {
        let db = Db::open_memory().await.unwrap();
        libraries::insert(&db, "lib", "Disk", "", "X:\\").await.unwrap();
        let t = "2026-01-01T00:00:00Z";
        for (rel, folder, hash) in [
            ("Köy\\a.jpg", "Köy", "a"),
            ("Köy\\Nişan\\b.jpg", "Köy\\Nişan", "b"),
            ("Köyde\\c.jpg", "Köyde", "c"),
        ] {
            let m = media::ScannedMedia { hash: hash.into(), ..Default::default() };
            media::upsert_file(&db, "lib", rel, folder, 1, 1, t, &m).await.unwrap();
        }
        let koy = get_or_create(&db, "event", "  Köy   Gezisi ").await.unwrap();
        assert_eq!(get_or_create(&db, "event", "KÖY GEZİSİ").await.unwrap(), koy, "Turkish-insensitive match");

        assert_eq!(tag_folder(&db, koy, "lib", "Köy", false).await.unwrap(), 1);
        // "Köyde" must not match the "Köy\" prefix.
        assert_eq!(tag_folder(&db, koy, "lib", "Köy", true).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn suggest_merge_and_parents() {
        let db = Db::open_memory().await.unwrap();
        let place = 2;
        let koy = get_or_create_in(&db, place, "Köy").await.unwrap();
        let koy2 = get_or_create_in(&db, place, "Köy Evi").await.unwrap();
        let sivas = get_or_create_in(&db, place, "Sivas").await.unwrap();
        let nisan = get_or_create(&db, "event", "Nişan 2022").await.unwrap();

        let names = |v: Vec<Tag>| v.into_iter().map(|t| t.name).collect::<Vec<_>>();
        assert_eq!(names(suggest(&db, "KÖ", None, 10).await.unwrap()).len(), 2);
        assert_eq!(names(suggest(&db, "nis 20", None, 10).await.unwrap()), vec!["Nişan 2022"]);
        assert_eq!(names(suggest(&db, "koy", Some(3), 10).await.unwrap()), Vec::<String>::new());

        set_parent(&db, koy, Some(sivas)).await.unwrap();
        assert!(matches!(set_parent(&db, sivas, Some(koy)).await, Err(CoreError::Invalid("tag_cycle"))));
        assert!(matches!(rename(&db, koy2, "köy").await, Err(CoreError::Conflict("tag_exists"))));

        merge(&db, koy2, koy).await.unwrap();
        assert!(get(&db, koy2).await.unwrap().is_none());
        assert_eq!(get(&db, koy).await.unwrap().unwrap().parent_name.as_deref(), Some("Sivas"));
        let _ = nisan;
    }

    #[tokio::test]
    async fn custom_tag_types() {
        let db = Db::open_memory().await.unwrap();
        let id = create_type(&db, "Mevsim", Some("#A0B0C0")).await.unwrap();
        assert!(create_type(&db, "Renk", Some("kirmizi")).await.is_err());
        get_or_create_in(&db, id, "Kış").await.unwrap();
        assert!(matches!(delete_type(&db, id).await, Err(CoreError::Invalid("tag_type_not_empty"))));
        assert!(matches!(delete_type(&db, 1).await, Err(CoreError::Invalid("builtin_tag_type"))));
        let types = list_types(&db).await.unwrap();
        assert_eq!(types.last().unwrap().color.as_deref(), Some("#a0b0c0"));
    }
}
