//! Admin media catalogue: filtered listing, details and bulk operations.

use serde::{Deserialize, Serialize};
use sqlx::{QueryBuilder, Sqlite};

use crate::{access, text::normalize, CoreError, CoreResult, Db};

/// Filters shared by the list and "select all matching" endpoints.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct MediaFilter {
    /// Words matched against tag names and file paths.
    pub q: Option<String>,
    /// Comma separated tag ids; media must have all of them.
    pub tags: Option<String>,
    pub category: Option<i64>,
    pub uncategorized: bool,
    pub untagged: bool,
    pub library: Option<String>,
    /// Folder (relative to the library root), including sub-folders.
    pub folder: Option<String>,
    /// 'photo' | 'video'
    pub kind: Option<String>,
    pub year: Option<i32>,
    /// 'private' | 'published' | 'hidden' | 'all' (default: everything not hidden)
    pub visibility: Option<String>,
    /// Published to this profile.
    pub profile: Option<i64>,
    /// 'taken_desc' (default) | 'taken_asc' | 'added_desc'
    pub sort: Option<String>,
}

fn push_where(qb: &mut QueryBuilder<'_, Sqlite>, f: &MediaFilter) {
    qb.push(" WHERE 1 = 1");
    match f.visibility.as_deref() {
        Some("hidden") => {
            qb.push(" AND m.is_hidden = 1");
        }
        Some("all") => {}
        Some("private") => {
            qb.push(" AND m.is_hidden = 0 AND NOT EXISTS (SELECT 1 FROM media_access ma WHERE ma.media_id = m.id)");
        }
        Some("published") => {
            qb.push(" AND m.is_hidden = 0 AND EXISTS (SELECT 1 FROM media_access ma WHERE ma.media_id = m.id)");
        }
        _ => {
            qb.push(" AND m.is_hidden = 0");
        }
    }
    if let Some(q) = f.q.as_deref() {
        for word in normalize(q).split(' ').filter(|w| !w.is_empty()) {
            let raw = q
                .split_whitespace()
                .find(|r| normalize(r) == word)
                .map(str::to_lowercase)
                .unwrap_or_else(|| word.to_string());
            qb.push(" AND (EXISTS (SELECT 1 FROM media_tags mt JOIN tags t ON t.id = mt.tag_id WHERE mt.media_id = m.id AND (t.name_norm LIKE ")
                .push_bind(format!("{word}%"))
                .push(" OR t.name_norm LIKE ")
                .push_bind(format!("% {word}%"))
                .push(")) OR EXISTS (SELECT 1 FROM media_files f WHERE f.media_id = m.id AND (f.rel_path LIKE ")
                .push_bind(format!("%{raw}%"))
                .push(" OR f.rel_path LIKE ")
                .push_bind(format!("%{word}%"))
                .push(")))");
        }
    }
    for tag in f.tags.as_deref().unwrap_or("").split(',').filter_map(|t| t.trim().parse::<i64>().ok()) {
        qb.push(" AND EXISTS (SELECT 1 FROM media_tags mt WHERE mt.media_id = m.id AND mt.tag_id = ").push_bind(tag).push(")");
    }
    if let Some(c) = f.category {
        qb.push(" AND EXISTS (SELECT 1 FROM media_categories mc WHERE mc.media_id = m.id AND mc.category_id = ").push_bind(c).push(")");
    }
    if f.uncategorized {
        qb.push(" AND NOT EXISTS (SELECT 1 FROM media_categories mc WHERE mc.media_id = m.id)");
    }
    if f.untagged {
        qb.push(" AND NOT EXISTS (SELECT 1 FROM media_tags mt WHERE mt.media_id = m.id)");
    }
    if let Some(lib) = f.library.as_deref().filter(|l| !l.is_empty()) {
        qb.push(" AND EXISTS (SELECT 1 FROM media_files f WHERE f.media_id = m.id AND f.library_id = ").push_bind(lib.to_string());
        if let Some(folder) = f.folder.as_deref().filter(|d| !d.is_empty()) {
            let prefix = format!("{folder}\\");
            qb.push(" AND (f.folder_rel = ")
                .push_bind(folder.to_string())
                .push(" OR substr(f.folder_rel, 1, ")
                .push_bind(prefix.chars().count() as i64)
                .push(") = ")
                .push_bind(prefix)
                .push(")");
        }
        qb.push(")");
    }
    if let Some(kind) = f.kind.as_deref().filter(|k| matches!(*k, "photo" | "video")) {
        qb.push(" AND m.kind = ").push_bind(kind.to_string());
    }
    if let Some(year) = f.year {
        qb.push(" AND substr(m.taken_at, 1, 4) = ").push_bind(format!("{year:04}"));
    }
    if let Some(p) = f.profile {
        qb.push(" AND EXISTS (SELECT 1 FROM media_access ma WHERE ma.media_id = m.id AND ma.profile_id = ").push_bind(p).push(")");
    }
}

fn push_order(qb: &mut QueryBuilder<'_, Sqlite>, f: &MediaFilter) {
    qb.push(match f.sort.as_deref() {
        Some("taken_asc") => " ORDER BY m.taken_at IS NULL, m.taken_at, m.id",
        Some("added_desc") => " ORDER BY m.id DESC",
        _ => " ORDER BY m.taken_at IS NULL, m.taken_at DESC, m.id DESC",
    });
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MediaItem {
    pub id: i64,
    pub kind: String,
    pub taken_at: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub thumb_ready: i64,
    pub is_hidden: bool,
    pub visibility_mode: String,
    pub published: bool,
    /// File name of the first (preferably present) file.
    pub name: Option<String>,
}

const ITEM_COLUMNS: &str = "m.id, m.kind, m.taken_at, m.width, m.height, m.duration_ms, m.thumb_ready, m.is_hidden,
    m.visibility_mode,
    EXISTS (SELECT 1 FROM media_access ma WHERE ma.media_id = m.id) AS published,
    (SELECT f.rel_path FROM media_files f WHERE f.media_id = m.id ORDER BY f.is_missing, f.id LIMIT 1) AS name";

#[derive(Debug, Serialize)]
pub struct MediaPage {
    pub total: i64,
    pub offset: i64,
    pub items: Vec<MediaItem>,
}

pub async fn list(db: &Db, f: &MediaFilter, offset: i64, limit: i64) -> CoreResult<MediaPage> {
    let mut count = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM media m");
    push_where(&mut count, f);
    let total: i64 = count.build_query_scalar().fetch_one(&db.pool).await?;

    let mut qb = QueryBuilder::<Sqlite>::new(format!("SELECT {ITEM_COLUMNS} FROM media m"));
    push_where(&mut qb, f);
    push_order(&mut qb, f);
    qb.push(" LIMIT ").push_bind(limit.clamp(1, 1000)).push(" OFFSET ").push_bind(offset.max(0));
    let mut items: Vec<MediaItem> = qb.build_query_as().fetch_all(&db.pool).await?;
    for item in &mut items {
        // Only the file name is useful in the grid.
        item.name = item.name.take().map(|p| p.rsplit('\\').next().unwrap_or(&p).to_string());
    }
    Ok(MediaPage { total, offset, items })
}

/// Every id matching the filter (for "select all"), capped.
pub async fn ids(db: &Db, f: &MediaFilter, cap: i64) -> CoreResult<Vec<i64>> {
    let mut qb = QueryBuilder::<Sqlite>::new("SELECT m.id FROM media m");
    push_where(&mut qb, f);
    push_order(&mut qb, f);
    qb.push(" LIMIT ").push_bind(cap);
    Ok(qb.build_query_scalar().fetch_all(&db.pool).await?)
}

#[derive(Debug, Serialize)]
pub struct MediaFile {
    pub library_id: String,
    pub library_name: String,
    pub library_online: bool,
    pub rel_path: String,
    pub size: i64,
    pub is_missing: bool,
}

#[derive(Debug, Serialize)]
pub struct MediaTagRef {
    pub id: i64,
    pub name: String,
    pub type_code: String,
    pub type_name: String,
    pub color: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MediaDetail {
    #[serde(flatten)]
    pub item: MediaItem,
    pub camera: Option<String>,
    pub taken_at_source: Option<String>,
    pub gps_lat: Option<f64>,
    pub gps_lon: Option<f64>,
    pub files: Vec<MediaFile>,
    pub tags: Vec<MediaTagRef>,
    pub categories: Vec<(i64, String)>,
    /// Own shares (used when visibility_mode = custom).
    pub share_profile_ids: Vec<i64>,
    /// Effective audience.
    pub access_profile_ids: Vec<i64>,
}

pub async fn detail(db: &Db, id: i64) -> CoreResult<Option<MediaDetail>> {
    let item: Option<MediaItem> = sqlx::query_as(&format!("SELECT {ITEM_COLUMNS} FROM media m WHERE m.id = ?"))
        .bind(id)
        .fetch_optional(&db.pool)
        .await?;
    let Some(item) = item else { return Ok(None) };
    let (camera, taken_at_source, gps_lat, gps_lon): (Option<String>, Option<String>, Option<f64>, Option<f64>) =
        sqlx::query_as("SELECT camera, taken_at_source, gps_lat, gps_lon FROM media WHERE id = ?")
            .bind(id)
            .fetch_one(&db.pool)
            .await?;
    let files = sqlx::query_as::<_, (String, String, bool, String, i64, bool)>(
        "SELECT l.id, l.name, l.is_online, f.rel_path, f.size, f.is_missing
         FROM media_files f JOIN libraries l ON l.id = f.library_id
         WHERE f.media_id = ? ORDER BY f.is_missing, f.rel_path",
    )
    .bind(id)
    .fetch_all(&db.pool)
    .await?
    .into_iter()
    .map(|(library_id, library_name, library_online, rel_path, size, is_missing)| MediaFile {
        library_id,
        library_name,
        library_online,
        rel_path,
        size,
        is_missing,
    })
    .collect();
    let tags = sqlx::query_as::<_, (i64, String, String, String, Option<String>)>(
        "SELECT t.id, t.name, tt.code, tt.name, tt.color
         FROM media_tags mt JOIN tags t ON t.id = mt.tag_id JOIN tag_types tt ON tt.id = t.type_id
         WHERE mt.media_id = ? ORDER BY tt.sort_order, t.name COLLATE NOCASE",
    )
    .bind(id)
    .fetch_all(&db.pool)
    .await?
    .into_iter()
    .map(|(id, name, type_code, type_name, color)| MediaTagRef { id, name, type_code, type_name, color })
    .collect();
    let categories = sqlx::query_as(
        "SELECT c.id, c.name FROM media_categories mc JOIN categories c ON c.id = mc.category_id
         WHERE mc.media_id = ? ORDER BY c.name COLLATE NOCASE",
    )
    .bind(id)
    .fetch_all(&db.pool)
    .await?;
    let share_profile_ids =
        sqlx::query_scalar("SELECT profile_id FROM media_shares WHERE media_id = ? ORDER BY profile_id").bind(id).fetch_all(&db.pool).await?;
    let access_profile_ids =
        sqlx::query_scalar("SELECT profile_id FROM media_access WHERE media_id = ? ORDER BY profile_id").bind(id).fetch_all(&db.pool).await?;
    Ok(Some(MediaDetail {
        item,
        camera,
        taken_at_source,
        gps_lat,
        gps_lon,
        files,
        tags,
        categories,
        share_profile_ids,
        access_profile_ids,
    }))
}

// ---------------------------------------------------------------------------
// Bulk operations
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum BulkOp {
    AddTags { tag_ids: Vec<i64> },
    RemoveTags { tag_ids: Vec<i64> },
    /// "Kopyala / aktar": also add to the category.
    AddToCategory { category_id: i64 },
    /// "Taşı": add to the category and leave `from_category_id` (or every other category).
    MoveToCategory { category_id: i64, from_category_id: Option<i64> },
    RemoveFromCategory { category_id: i64 },
    /// mode: 'inherit' (from categories) | 'private' | 'profiles'
    SetVisibility { mode: String, #[serde(default)] profile_ids: Vec<i64> },
    SetHidden { hidden: bool },
    /// "YYYY-MM-DD" or "YYYY-MM-DDTHH:MM[:SS]"
    SetTakenAt { taken_at: String },
}

pub const MAX_BULK: usize = 50_000;

fn normalize_taken_at(s: &str) -> CoreResult<String> {
    let s = s.trim().replace(' ', "T");
    let (date, time) = s.split_once('T').unwrap_or((&s, "00:00:00"));
    let d: Vec<u32> = date.split('-').filter_map(|p| p.parse().ok()).collect();
    let t: Vec<u32> = time.split(':').filter_map(|p| p.parse().ok()).collect();
    let ok = d.len() == 3
        && (1900..=2100).contains(&d[0])
        && (1..=12).contains(&d[1])
        && (1..=31).contains(&d[2])
        && (2..=3).contains(&t.len())
        && t[0] < 24
        && t[1] < 60
        && t.get(2).copied().unwrap_or(0) < 60;
    if !ok {
        return Err(CoreError::Invalid("invalid_date"));
    }
    Ok(format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", d[0], d[1], d[2], t[0], t[1], t.get(2).copied().unwrap_or(0)))
}

/// Applies `op` to `media_ids`; returns the number of rows changed.
pub async fn bulk(db: &Db, media_ids: &[i64], op: &BulkOp) -> CoreResult<u64> {
    if media_ids.is_empty() {
        return Ok(0);
    }
    if media_ids.len() > MAX_BULK {
        return Err(CoreError::Invalid("too_many_items"));
    }
    let ids = serde_json::to_string(media_ids).unwrap_or_default();
    let mut tx = db.pool.begin().await?;
    let mut changed = 0u64;
    let mut affects_access = false;

    match op {
        BulkOp::AddTags { tag_ids } => {
            for t in tag_ids {
                changed += sqlx::query(
                    "INSERT OR IGNORE INTO media_tags (media_id, tag_id, source)
                     SELECT m.id, t.id, 'manual' FROM media m, tags t
                     WHERE t.id = ? AND m.id IN (SELECT value FROM json_each(?))",
                )
                .bind(t)
                .bind(&ids)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            }
        }
        BulkOp::RemoveTags { tag_ids } => {
            let tags = serde_json::to_string(tag_ids).unwrap_or_default();
            changed = sqlx::query(
                "DELETE FROM media_tags WHERE tag_id IN (SELECT value FROM json_each(?))
                   AND media_id IN (SELECT value FROM json_each(?))",
            )
            .bind(tags)
            .bind(&ids)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
        BulkOp::AddToCategory { category_id } | BulkOp::MoveToCategory { category_id, .. } => {
            let found: Option<i64> =
                sqlx::query_scalar("SELECT id FROM categories WHERE id = ?").bind(category_id).fetch_optional(&mut *tx).await?;
            found.ok_or(CoreError::NotFound)?;
            if let BulkOp::MoveToCategory { from_category_id, .. } = op {
                let removed = match from_category_id {
                    Some(from) => sqlx::query(
                        "DELETE FROM media_categories WHERE category_id = ? AND media_id IN (SELECT value FROM json_each(?))",
                    )
                    .bind(from)
                    .bind(&ids),
                    None => sqlx::query(
                        "DELETE FROM media_categories WHERE category_id <> ? AND media_id IN (SELECT value FROM json_each(?))",
                    )
                    .bind(category_id)
                    .bind(&ids),
                }
                .execute(&mut *tx)
                .await?;
                changed += removed.rows_affected();
            }
            changed += sqlx::query(
                "INSERT OR IGNORE INTO media_categories (media_id, category_id)
                 SELECT m.id, ? FROM media m WHERE m.id IN (SELECT value FROM json_each(?))",
            )
            .bind(category_id)
            .bind(&ids)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            affects_access = true;
        }
        BulkOp::RemoveFromCategory { category_id } => {
            changed = sqlx::query(
                "DELETE FROM media_categories WHERE category_id = ? AND media_id IN (SELECT value FROM json_each(?))",
            )
            .bind(category_id)
            .bind(&ids)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            affects_access = true;
        }
        BulkOp::SetVisibility { mode, profile_ids } => {
            let db_mode = match mode.as_str() {
                "inherit" => "inherit",
                "private" | "profiles" => "custom",
                _ => return Err(CoreError::Invalid("invalid_visibility")),
            };
            changed = sqlx::query("UPDATE media SET visibility_mode = ? WHERE id IN (SELECT value FROM json_each(?))")
                .bind(db_mode)
                .bind(&ids)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            sqlx::query("DELETE FROM media_shares WHERE media_id IN (SELECT value FROM json_each(?))")
                .bind(&ids)
                .execute(&mut *tx)
                .await?;
            if mode == "profiles" {
                for p in profile_ids {
                    sqlx::query(
                        "INSERT OR IGNORE INTO media_shares (media_id, profile_id)
                         SELECT m.id, p.id FROM media m, profiles p
                         WHERE p.id = ? AND m.id IN (SELECT value FROM json_each(?))",
                    )
                    .bind(p)
                    .bind(&ids)
                    .execute(&mut *tx)
                    .await?;
                }
            }
            affects_access = true;
        }
        BulkOp::SetHidden { hidden } => {
            changed = sqlx::query("UPDATE media SET is_hidden = ? WHERE id IN (SELECT value FROM json_each(?))")
                .bind(hidden)
                .bind(&ids)
                .execute(&mut *tx)
                .await?
                .rows_affected();
            affects_access = true;
        }
        BulkOp::SetTakenAt { taken_at } => {
            changed = sqlx::query(
                "UPDATE media SET taken_at = ?, taken_at_source = 'manual' WHERE id IN (SELECT value FROM json_each(?))",
            )
            .bind(normalize_taken_at(taken_at)?)
            .bind(&ids)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        }
    }
    if affects_access {
        access::recompute(&mut tx, media_ids).await?;
    }
    tx.commit().await?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        access::can_view,
        categories, libraries, media, profiles, tags,
        users::{self, NewUser, Role},
    };

    async fn seed(db: &Db) -> Vec<i64> {
        libraries::insert(db, "lib", "Disk", "", "X:\\").await.unwrap();
        let mut ids = vec![];
        for (i, (rel, taken)) in [
            ("Köy\\IMG_1.jpg", "2022-07-11T14:30:00"),
            ("Köy\\IMG_2.jpg", "2022-07-12T10:00:00"),
            ("Düğün\\DSC_1.jpg", "2023-05-01T18:00:00"),
        ]
        .iter()
        .enumerate()
        {
            let m = media::ScannedMedia {
                hash: format!("h{i}"),
                kind: Some(media::MediaKind::Photo),
                taken_at: Some(taken.to_string()),
                ..Default::default()
            };
            let folder = rel.rsplit_once('\\').unwrap().0;
            media::upsert_file(db, "lib", rel, folder, 1, 1, "2026-01-01T00:00:00.000Z", &m).await.unwrap();
            ids.push(sqlx::query_scalar("SELECT id FROM media WHERE hash = ?").bind(format!("h{i}")).fetch_one(&db.pool).await.unwrap());
        }
        ids
    }

    #[tokio::test]
    async fn filters_and_bulk_tags() {
        let db = Db::open_memory().await.unwrap();
        let ids = seed(&db).await;
        let f = |q: &str| MediaFilter { q: Some(q.into()), ..Default::default() };

        assert_eq!(list(&db, &MediaFilter::default(), 0, 100).await.unwrap().total, 3);
        assert_eq!(list(&db, &f("köy"), 0, 100).await.unwrap().total, 2, "path match, Turkish");
        assert_eq!(list(&db, &MediaFilter { year: Some(2023), ..Default::default() }, 0, 100).await.unwrap().total, 1);
        let folder = MediaFilter { library: Some("lib".into()), folder: Some("Düğün".into()), ..Default::default() };
        assert_eq!(ids_of(&db, &folder).await, vec![ids[2]]);

        let nisan = tags::get_or_create(&db, "event", "Nişan 2022").await.unwrap();
        bulk(&db, &ids[..2], &BulkOp::AddTags { tag_ids: vec![nisan] }).await.unwrap();
        assert_eq!(list(&db, &f("nisan"), 0, 100).await.unwrap().total, 2, "tag match");
        let by_tag = MediaFilter { tags: Some(nisan.to_string()), ..Default::default() };
        assert_eq!(list(&db, &by_tag, 0, 100).await.unwrap().total, 2);
        assert_eq!(list(&db, &MediaFilter { untagged: true, ..Default::default() }, 0, 100).await.unwrap().total, 1);

        // Newest first by default, grid gets only the file name.
        let page = list(&db, &MediaFilter::default(), 0, 2).await.unwrap();
        assert_eq!(page.items[0].name.as_deref(), Some("DSC_1.jpg"));

        bulk(&db, &ids[..1], &BulkOp::SetTakenAt { taken_at: "2021-03-04".into() }).await.unwrap();
        assert!(bulk(&db, &ids[..1], &BulkOp::SetTakenAt { taken_at: "2021-13-04".into() }).await.is_err());
    }

    async fn ids_of(db: &Db, f: &MediaFilter) -> Vec<i64> {
        ids(db, f, 100).await.unwrap()
    }

    #[tokio::test]
    async fn publishing_rules() {
        let db = Db::open_memory().await.unwrap();
        let ids = seed(&db).await;
        let ahmet = users::create(&db, NewUser { username: "ahmet".into(), display_name: "Ahmet".into(), password_hash: "x".into(), role: Role::Member })
            .await
            .unwrap();
        let ayse = users::create(&db, NewUser { username: "ayse".into(), display_name: "Ayşe".into(), password_hash: "x".into(), role: Role::Member })
            .await
            .unwrap();
        let yakin = profiles::create(&db, "Yakın Aile", None).await.unwrap();
        profiles::set_members(&db, yakin, &[ahmet.id]).await.unwrap();

        // Category published to "Yakın Aile": Ahmet sees its media, Ayşe does not.
        let koy = categories::create(&db, None, "Köy", None).await.unwrap();
        bulk(&db, &ids[..2], &BulkOp::AddToCategory { category_id: koy }).await.unwrap();
        assert!(!can_view(&db, &ahmet, ids[0]).await.unwrap(), "categories start private");
        categories::set_shares(&db, koy, &[yakin], false).await.unwrap();
        assert!(can_view(&db, &ahmet, ids[0]).await.unwrap());
        assert!(!can_view(&db, &ayse, ids[0]).await.unwrap());

        // One photo made private although its category is published.
        bulk(&db, &ids[1..2], &BulkOp::SetVisibility { mode: "private".into(), profile_ids: vec![] }).await.unwrap();
        assert!(!can_view(&db, &ahmet, ids[1]).await.unwrap());
        // …and another published to everyone on its own.
        bulk(&db, &ids[2..3], &BulkOp::SetVisibility { mode: "profiles".into(), profile_ids: vec![1] }).await.unwrap();
        assert!(can_view(&db, &ayse, ids[2]).await.unwrap());

        // Hidden beats everything.
        bulk(&db, &ids[..1], &BulkOp::SetHidden { hidden: true }).await.unwrap();
        assert!(!can_view(&db, &ahmet, ids[0]).await.unwrap());
        bulk(&db, &ids[..1], &BulkOp::SetHidden { hidden: false }).await.unwrap();

        // Moving out of the published category makes it private again.
        let arsiv = categories::create(&db, None, "Arşiv", None).await.unwrap();
        bulk(&db, &ids[..1], &BulkOp::MoveToCategory { category_id: arsiv, from_category_id: Some(koy) }).await.unwrap();
        assert!(!can_view(&db, &ahmet, ids[0]).await.unwrap());
        let published = MediaFilter { visibility: Some("published".into()), ..Default::default() };
        assert_eq!(ids_of(&db, &published).await, vec![ids[2]]);
        let for_yakin = MediaFilter { profile: Some(yakin), ..Default::default() };
        assert!(ids_of(&db, &for_yakin).await.is_empty());

        // Deleting the category recomputes too; publishing the new one to everyone.
        categories::set_shares(&db, arsiv, &[1], false).await.unwrap();
        assert!(can_view(&db, &ayse, ids[0]).await.unwrap());
        categories::delete(&db, arsiv).await.unwrap();
        assert!(!can_view(&db, &ayse, ids[0]).await.unwrap());
    }
}
