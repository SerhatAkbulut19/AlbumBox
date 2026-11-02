//! Grouped search ("köy, nişan" → groups per event, with facets).
//!
//! Every word must match (AND). A word matches a media item when
//! - one of its tags (or an ancestor tag: "Sivas" finds "Köy" under Sivas)
//!   has a word starting with it, or
//! - a folder name of one of its files has a word starting with it, or
//! - it is a year and the item was taken that year.
//!
//! Results are limited to what the viewer may see. Matching ids are put into a
//! temporary table once; groups, facets and pages are computed from it.

use serde::{Deserialize, Serialize};
use sqlx::{pool::PoolConnection, QueryBuilder, Sqlite};

use crate::{
    text::normalize,
    users::{Role, User},
    CoreResult, Db,
};

/// Who is searching.
#[derive(Debug, Clone, Copy)]
pub struct Viewer {
    pub user_id: i64,
    pub admin: bool,
}

impl Viewer {
    pub fn of(user: &User) -> Self {
        Self { user_id: user.id, admin: user.role == Role::Admin }
    }

    /// `AND <alias> visible to the viewer`.
    pub fn push_visible(&self, qb: &mut QueryBuilder<'_, Sqlite>, alias: &str) {
        qb.push(format!(" AND {alias}.is_hidden = 0"));
        if !self.admin {
            qb.push(format!(
                " AND EXISTS (SELECT 1 FROM media_access ma JOIN user_profiles up ON up.profile_id = ma.profile_id
                   WHERE ma.media_id = {alias}.id AND up.user_id = "
            ))
            .push_bind(self.user_id)
            .push(")");
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SearchQuery {
    /// Words separated by spaces or commas; all must match.
    pub q: Option<String>,
    /// Comma separated tag ids picked from facets (all must match).
    pub tags: Option<String>,
    pub year: Option<i32>,
    pub category: Option<i64>,
    /// 'event' (default) | 'year'
    pub group: Option<String>,
}

impl SearchQuery {
    pub fn words(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for w in normalize(&self.q.clone().unwrap_or_default().replace([',', ';'], " ")).split(' ') {
            let w: String = w.chars().filter(|c| c.is_alphanumeric()).collect();
            if !w.is_empty() && !out.contains(&w) {
                out.push(w);
            }
        }
        out
    }

    fn tag_ids(&self) -> Vec<i64> {
        self.tags.as_deref().unwrap_or("").split(',').filter_map(|t| t.trim().parse().ok()).collect()
    }

    pub fn by_year(&self) -> bool {
        self.group.as_deref() == Some("year")
    }
}

fn year_of(word: &str) -> Option<String> {
    (word.len() == 4 && word.chars().all(|c| c.is_ascii_digit()) && ("1900".."2101").contains(&word))
        .then(|| word.to_string())
}

/// Fills `temp.hits` with the matching, visible media.
async fn fill_hits(conn: &mut PoolConnection<Sqlite>, viewer: Viewer, q: &SearchQuery) -> CoreResult<()> {
    sqlx::query("DROP TABLE IF EXISTS temp.hits").execute(&mut **conn).await?;
    sqlx::query("CREATE TEMP TABLE hits (id INTEGER PRIMARY KEY, taken_at TEXT, thumb_ready INTEGER)")
        .execute(&mut **conn)
        .await?;
    let mut qb = QueryBuilder::<Sqlite>::new("INSERT INTO hits SELECT m.id, m.taken_at, m.thumb_ready FROM media m WHERE 1 = 1");
    viewer.push_visible(&mut qb, "m");
    for word in q.words() {
        qb.push(
            " AND (EXISTS (SELECT 1 FROM media_tags mt WHERE mt.media_id = m.id AND mt.tag_id IN (
                WITH RECURSIVE x(id) AS (
                    SELECT rowid FROM tags_fts WHERE tags_fts MATCH ",
        )
        .push_bind(format!("\"{word}\"*"))
        .push(
            " UNION SELECT t.id FROM tags t JOIN x ON t.parent_id = x.id)
                SELECT id FROM x))
             OR EXISTS (SELECT 1 FROM media_files f WHERE f.media_id = m.id AND f.is_missing = 0
                        AND (' ' || f.folder_norm) LIKE ",
        )
        .push_bind(format!("% {word}%"))
        .push(")");
        if let Some(y) = year_of(&word) {
            qb.push(" OR substr(m.taken_at, 1, 4) = ").push_bind(y);
        }
        qb.push(")");
    }
    for tag in q.tag_ids() {
        qb.push(" AND EXISTS (SELECT 1 FROM media_tags mt WHERE mt.media_id = m.id AND mt.tag_id = ").push_bind(tag).push(")");
    }
    if let Some(y) = q.year {
        qb.push(" AND substr(m.taken_at, 1, 4) = ").push_bind(format!("{y:04}"));
    }
    if let Some(c) = q.category {
        qb.push(" AND EXISTS (SELECT 1 FROM media_categories mc WHERE mc.media_id = m.id AND mc.category_id = ").push_bind(c).push(")");
    }
    qb.build().execute(&mut **conn).await?;
    Ok(())
}

async fn drop_hits(conn: &mut PoolConnection<Sqlite>) {
    let _ = sqlx::query("DROP TABLE IF EXISTS temp.hits").execute(&mut **conn).await;
}

#[derive(Debug, Clone, Serialize)]
pub struct TagRef {
    pub id: i64,
    pub name: String,
    pub type_code: String,
    pub color: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Term {
    pub word: String,
    /// Tags this word matched among the results (for the chips above the results).
    pub tags: Vec<TagRef>,
}

#[derive(Debug, Serialize)]
pub struct Group {
    /// Event tag id, a year, or "none" (no event / no date).
    pub key: String,
    /// `None` for the "none" group (the UI names it).
    pub name: Option<String>,
    pub count: i64,
    pub cover_media_id: Option<i64>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Facet {
    pub id: i64,
    pub name: String,
    pub count: i64,
}

#[derive(Debug, Default, Serialize)]
pub struct Facets {
    pub persons: Vec<Facet>,
    pub places: Vec<Facet>,
    pub events: Vec<Facet>,
    pub others: Vec<Facet>,
    pub years: Vec<(String, i64)>,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub total: i64,
    pub terms: Vec<Term>,
    pub groups: Vec<Group>,
    pub facets: Facets,
}

const FACET_LIMIT: usize = 15;

pub async fn search(db: &Db, viewer: Viewer, q: &SearchQuery) -> CoreResult<SearchResult> {
    let mut conn = db.pool.acquire().await?;
    fill_hits(&mut conn, viewer, q).await?;
    let result = compute(&mut conn, q).await;
    drop_hits(&mut conn).await;
    result
}

async fn compute(conn: &mut PoolConnection<Sqlite>, q: &SearchQuery) -> CoreResult<SearchResult> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM hits").fetch_one(&mut **conn).await?;

    let mut terms = Vec::new();
    for word in q.words() {
        let tags: Vec<(i64, String, String, Option<String>)> = sqlx::query_as(
            "SELECT t.id, t.name, tt.code, tt.color FROM tags t JOIN tag_types tt ON tt.id = t.type_id
             WHERE t.id IN (SELECT rowid FROM tags_fts WHERE tags_fts MATCH ?)
               AND EXISTS (SELECT 1 FROM media_tags mt JOIN hits h ON h.id = mt.media_id WHERE mt.tag_id = t.id)
             ORDER BY (SELECT COUNT(*) FROM media_tags mt JOIN hits h ON h.id = mt.media_id WHERE mt.tag_id = t.id) DESC
             LIMIT 5",
        )
        .bind(format!("\"{word}\"*"))
        .fetch_all(&mut **conn)
        .await?;
        terms.push(Term {
            word,
            tags: tags.into_iter().map(|(id, name, type_code, color)| TagRef { id, name, type_code, color }).collect(),
        });
    }

    let mut groups: Vec<Group> = Vec::new();
    if q.by_year() {
        let rows: Vec<(Option<String>, i64, Option<String>, Option<String>, Option<i64>)> = sqlx::query_as(
            "SELECT substr(h.taken_at, 1, 4) AS y, COUNT(*), MIN(h.taken_at), MAX(h.taken_at),
                    (SELECT h2.id FROM hits h2 WHERE substr(h2.taken_at, 1, 4) IS substr(h.taken_at, 1, 4) AND h2.thumb_ready = 1
                     ORDER BY h2.taken_at LIMIT 1)
             FROM hits h GROUP BY y ORDER BY y IS NULL, y DESC",
        )
        .fetch_all(&mut **conn)
        .await?;
        for (year, count, from, to, cover) in rows {
            groups.push(Group {
                key: year.clone().unwrap_or_else(|| "none".into()),
                name: year,
                count,
                cover_media_id: cover,
                date_from: from,
                date_to: to,
            });
        }
    } else {
        let rows: Vec<(i64, String, i64, Option<String>, Option<String>, Option<i64>)> = sqlx::query_as(
            "SELECT t.id, t.name, COUNT(*), MIN(h.taken_at), MAX(h.taken_at),
                    (SELECT h2.id FROM hits h2 JOIN media_tags mt2 ON mt2.media_id = h2.id
                     WHERE mt2.tag_id = t.id AND h2.thumb_ready = 1 ORDER BY h2.taken_at LIMIT 1)
             FROM hits h
             JOIN media_tags mt ON mt.media_id = h.id
             JOIN tags t ON t.id = mt.tag_id
             JOIN tag_types tt ON tt.id = t.type_id AND tt.code = 'event'
             GROUP BY t.id
             ORDER BY MAX(h.taken_at) IS NULL, MAX(h.taken_at) DESC",
        )
        .fetch_all(&mut **conn)
        .await?;
        for (id, name, count, from, to, cover) in rows {
            groups.push(Group { key: id.to_string(), name: Some(name), count, cover_media_id: cover, date_from: from, date_to: to });
        }
        let (count, from, to, cover): (i64, Option<String>, Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT COUNT(*), MIN(h.taken_at), MAX(h.taken_at),
                    (SELECT h2.id FROM hits h2 WHERE h2.thumb_ready = 1 AND NOT EXISTS (
                        SELECT 1 FROM media_tags mt JOIN tags t ON t.id = mt.tag_id JOIN tag_types tt ON tt.id = t.type_id
                        WHERE mt.media_id = h2.id AND tt.code = 'event') ORDER BY h2.taken_at LIMIT 1)
             FROM hits h WHERE NOT EXISTS (
                SELECT 1 FROM media_tags mt JOIN tags t ON t.id = mt.tag_id JOIN tag_types tt ON tt.id = t.type_id
                WHERE mt.media_id = h.id AND tt.code = 'event')",
        )
        .fetch_one(&mut **conn)
        .await?;
        if count > 0 {
            groups.push(Group { key: "none".into(), name: None, count, cover_media_id: cover, date_from: from, date_to: to });
        }
    }

    let mut facets = Facets::default();
    let rows: Vec<(String, i64, String, i64)> = sqlx::query_as(
        "SELECT tt.code, t.id, t.name, COUNT(*) FROM hits h
         JOIN media_tags mt ON mt.media_id = h.id
         JOIN tags t ON t.id = mt.tag_id
         JOIN tag_types tt ON tt.id = t.type_id
         GROUP BY t.id ORDER BY COUNT(*) DESC, t.name COLLATE NOCASE",
    )
    .fetch_all(&mut **conn)
    .await?;
    for (code, id, name, count) in rows {
        let bucket = match code.as_str() {
            "person" => &mut facets.persons,
            "place" => &mut facets.places,
            "event" => &mut facets.events,
            _ => &mut facets.others,
        };
        if bucket.len() < FACET_LIMIT {
            bucket.push(Facet { id, name, count });
        }
    }
    facets.years = sqlx::query_as(
        "SELECT substr(taken_at, 1, 4) AS y, COUNT(*) FROM hits WHERE taken_at IS NOT NULL GROUP BY y ORDER BY y DESC",
    )
    .fetch_all(&mut **conn)
    .await?;

    Ok(SearchResult { total, terms, groups, facets })
}

/// Media in the grid / viewer for signed-in users.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ClientItem {
    pub id: i64,
    pub kind: String,
    pub taken_at: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub thumb_ready: i64,
}

#[derive(Debug, Serialize)]
pub struct ClientPage {
    pub total: i64,
    pub offset: i64,
    pub items: Vec<ClientItem>,
}

pub const ITEM_COLUMNS: &str = "m.id, m.kind, m.taken_at, m.width, m.height, m.duration_ms, m.thumb_ready";

/// `WHERE …` restricting `hits h` to one group.
fn push_group_filter(qb: &mut QueryBuilder<'_, Sqlite>, group: Option<&str>, by_year: bool) {
    const NO_EVENT: &str = " AND NOT EXISTS (SELECT 1 FROM media_tags mt JOIN tags t ON t.id = mt.tag_id
        JOIN tag_types tt ON tt.id = t.type_id WHERE mt.media_id = h.id AND tt.code = 'event')";
    qb.push(" WHERE 1 = 1");
    match (group, by_year) {
        (None, _) => {}
        (Some("none"), true) => {
            qb.push(" AND h.taken_at IS NULL");
        }
        (Some(year), true) => {
            qb.push(" AND substr(h.taken_at, 1, 4) = ").push_bind(year.to_string());
        }
        (Some("none"), false) => {
            qb.push(NO_EVENT);
        }
        (Some(tag), false) => {
            qb.push(" AND EXISTS (SELECT 1 FROM media_tags mt WHERE mt.media_id = h.id AND mt.tag_id = ")
                .push_bind(tag.parse::<i64>().unwrap_or(-1))
                .push(")");
        }
    }
}

/// One group of a search (or all results when `group` is None).
pub async fn group_media(
    db: &Db,
    viewer: Viewer,
    q: &SearchQuery,
    group: Option<&str>,
    offset: i64,
    limit: i64,
) -> CoreResult<ClientPage> {
    let mut conn = db.pool.acquire().await?;
    fill_hits(&mut conn, viewer, q).await?;
    let result = async {
        let mut count = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM hits h");
        let mut page = QueryBuilder::<Sqlite>::new(format!("SELECT {ITEM_COLUMNS} FROM hits h JOIN media m ON m.id = h.id"));
        push_group_filter(&mut count, group, q.by_year());
        push_group_filter(&mut page, group, q.by_year());
        let total: i64 = count.build_query_scalar().fetch_one(&mut *conn).await?;
        // A group reads like a story (oldest first); "all results" newest first.
        page.push(if group.is_some() {
            " ORDER BY h.taken_at IS NULL, h.taken_at, h.id"
        } else {
            " ORDER BY h.taken_at IS NULL, h.taken_at DESC, h.id DESC"
        });
        page.push(" LIMIT ").push_bind(limit.clamp(1, 500)).push(" OFFSET ").push_bind(offset.max(0));
        let items: Vec<ClientItem> = page.build_query_as().fetch_all(&mut *conn).await?;
        CoreResult::Ok(ClientPage { total, offset, items })
    }
    .await;
    drop_hits(&mut conn).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog::{bulk, BulkOp},
        libraries, media, tags,
        users::{self, NewUser},
    };

    struct Seed {
        admin: Viewer,
        member: Viewer,
        ids: Vec<i64>,
    }

    /// 4 photos: Köy+Nişan 2022 (×2), Köy+Ramazan 2023, Düğün folder without tags.
    async fn seed(db: &Db) -> Seed {
        libraries::insert(db, "lib", "Disk", "", "X:\\").await.unwrap();
        let mut ids = vec![];
        for (i, (rel, taken)) in [
            ("Telefon\\a.jpg", "2022-08-12T19:00:00"),
            ("Telefon\\b.jpg", "2022-08-13T19:00:00"),
            ("Telefon\\c.jpg", "2023-04-22T10:00:00"),
            ("Düğün Ayşe\\d.jpg", "2021-06-01T15:00:00"),
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
        sqlx::query("UPDATE media SET thumb_ready = 1").execute(&db.pool).await.unwrap();
        let sivas = tags::get_or_create(db, "place", "Sivas").await.unwrap();
        let koy = tags::get_or_create(db, "place", "Köy").await.unwrap();
        tags::set_parent(db, koy, Some(sivas)).await.unwrap();
        let nisan = tags::get_or_create(db, "event", "Nişan 2022").await.unwrap();
        let ramazan = tags::get_or_create(db, "event", "Ramazan Bayramı 2023").await.unwrap();
        let ahmet = tags::get_or_create(db, "person", "Ahmet").await.unwrap();
        bulk(db, &ids[..3], &BulkOp::AddTags { tag_ids: vec![koy] }).await.unwrap();
        bulk(db, &ids[..2], &BulkOp::AddTags { tag_ids: vec![nisan, ahmet] }).await.unwrap();
        bulk(db, &ids[2..3], &BulkOp::AddTags { tag_ids: vec![ramazan] }).await.unwrap();

        let a = users::create(db, NewUser { username: "admin".into(), display_name: "A".into(), password_hash: "x".into(), role: Role::Admin })
            .await
            .unwrap();
        let m = users::create(db, NewUser { username: "uye".into(), display_name: "Ü".into(), password_hash: "x".into(), role: Role::Member })
            .await
            .unwrap();
        Seed { admin: Viewer::of(&a), member: Viewer::of(&m), ids }
    }

    fn q(text: &str) -> SearchQuery {
        SearchQuery { q: Some(text.into()), ..Default::default() }
    }

    #[tokio::test]
    async fn groups_by_event_with_facets() {
        let db = Db::open_memory().await.unwrap();
        let s = seed(&db).await;

        let r = search(&db, s.admin, &q("KÖY")).await.unwrap();
        assert_eq!(r.total, 3);
        let groups: Vec<_> = r.groups.iter().map(|g| (g.name.clone(), g.count)).collect();
        assert_eq!(groups, vec![(Some("Ramazan Bayramı 2023".into()), 1), (Some("Nişan 2022".into()), 2)]);
        assert_eq!(r.terms[0].tags[0].name, "Köy");
        assert_eq!(r.facets.persons[0].name, "Ahmet");
        assert_eq!(r.facets.years, vec![("2023".into(), 1), ("2022".into(), 2)]);

        // "köy, nişan": both words must match.
        let r = search(&db, s.admin, &q("köy, nişan")).await.unwrap();
        assert_eq!((r.total, r.groups.len()), (2, 1));

        // Parent tag: "sivas" finds media tagged "Köy".
        assert_eq!(search(&db, s.admin, &q("sivas")).await.unwrap().total, 3);
        // Folder names: "düğün" matches the untagged photo, which lands in the "none" group.
        let r = search(&db, s.admin, &q("dugun")).await.unwrap();
        assert_eq!((r.total, r.groups[0].key.as_str()), (1, "none"));
        // Years as words.
        assert_eq!(search(&db, s.admin, &q("köy 2023")).await.unwrap().total, 1);

        let by_year = SearchQuery { group: Some("year".into()), ..q("köy") };
        let r = search(&db, s.admin, &by_year).await.unwrap();
        assert_eq!(r.groups.iter().map(|g| g.key.as_str()).collect::<Vec<_>>(), vec!["2023", "2022"]);

        let nisan_key = search(&db, s.admin, &q("köy")).await.unwrap().groups[1].key.clone();
        let page = group_media(&db, s.admin, &q("köy"), Some(&nisan_key), 0, 10).await.unwrap();
        assert_eq!(page.items.iter().map(|i| i.id).collect::<Vec<_>>(), vec![s.ids[0], s.ids[1]], "oldest first");
        let all = group_media(&db, s.admin, &q("köy"), None, 0, 10).await.unwrap();
        assert_eq!(all.total, 3);
    }

    #[tokio::test]
    async fn members_only_find_published_media() {
        let db = Db::open_memory().await.unwrap();
        let s = seed(&db).await;
        assert_eq!(search(&db, s.member, &q("köy")).await.unwrap().total, 0);

        bulk(&db, &s.ids[2..3], &BulkOp::SetVisibility { mode: "profiles".into(), profile_ids: vec![1] }).await.unwrap();
        let r = search(&db, s.member, &q("köy")).await.unwrap();
        assert_eq!(r.total, 1);
        assert_eq!(r.groups[0].name.as_deref(), Some("Ramazan Bayramı 2023"));
        assert!(r.facets.persons.is_empty(), "tags of private media stay hidden");
        assert_eq!(group_media(&db, s.member, &q(""), None, 0, 10).await.unwrap().total, 1);
    }
}
