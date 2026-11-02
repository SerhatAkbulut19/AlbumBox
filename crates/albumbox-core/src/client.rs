//! Client (family member) views: home, albums, media info. Everything is
//! limited to what the viewer may see.

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use sqlx::{QueryBuilder, Sqlite};

use crate::{
    access,
    search::{ClientItem, ClientPage, TagRef, Viewer, ITEM_COLUMNS},
    users::User,
    CoreError, CoreResult, Db,
};

#[derive(Debug, Clone, Serialize)]
pub struct Album {
    pub id: i64,
    /// Parent album, or `None` when the parent is not visible to the viewer.
    pub parent_id: Option<i64>,
    pub name: String,
    pub description: Option<String>,
    /// Visible media directly in the album.
    pub media_count: i64,
    /// Visible media in the album and all visible sub-albums.
    pub total_count: i64,
    pub cover_media_id: Option<i64>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
}

/// Albums the viewer may open: published to one of their profiles, or holding
/// media they can see. Ordered like the admin tree.
pub async fn albums(db: &Db, viewer: Viewer) -> CoreResult<Vec<Album>> {
    let mut qb = QueryBuilder::<Sqlite>::new(
        "WITH vis AS (SELECT m.id, m.taken_at, m.thumb_ready FROM media m WHERE 1 = 1",
    );
    viewer.push_visible(&mut qb, "m");
    qb.push(
        ")
        SELECT c.id, c.parent_id, c.name, c.description,
               (SELECT COUNT(*) FROM media_categories mc JOIN vis v ON v.id = mc.media_id WHERE mc.category_id = c.id),
               COALESCE(
                   (SELECT v.id FROM vis v WHERE v.id = c.cover_media_id AND v.thumb_ready = 1),
                   (SELECT v.id FROM media_categories mc JOIN vis v ON v.id = mc.media_id
                    WHERE mc.category_id = c.id AND v.thumb_ready = 1 ORDER BY v.taken_at LIMIT 1)),
               (SELECT MIN(v.taken_at) FROM media_categories mc JOIN vis v ON v.id = mc.media_id WHERE mc.category_id = c.id),
               (SELECT MAX(v.taken_at) FROM media_categories mc JOIN vis v ON v.id = mc.media_id WHERE mc.category_id = c.id),
               EXISTS (SELECT 1 FROM category_shares cs JOIN user_profiles up ON up.profile_id = cs.profile_id
                       WHERE cs.category_id = c.id AND up.user_id = ",
    )
    .push_bind(viewer.user_id)
    .push(") FROM categories c ORDER BY c.sort_order, c.name COLLATE NOCASE");

    type Row = (i64, Option<i64>, String, Option<String>, i64, Option<i64>, Option<String>, Option<String>, bool);
    let rows: Vec<Row> = qb.build_query_as().fetch_all(&db.pool).await?;

    // Admins see every album; members see albums shared with them or holding media
    // they can see. A visible album under a hidden parent moves up to the nearest
    // visible ancestor (or the top level), so private album names never leak.
    let parent_of: HashMap<i64, Option<i64>> = rows.iter().map(|r| (r.0, r.1)).collect();
    let keep: HashSet<i64> = rows.iter().filter(|r| viewer.admin || r.8 || r.4 > 0).map(|r| r.0).collect();
    let mut list: Vec<Album> = rows
        .into_iter()
        .filter(|r| keep.contains(&r.0))
        .map(|r| {
            let mut parent = r.1;
            while let Some(p) = parent {
                if keep.contains(&p) {
                    break;
                }
                parent = parent_of.get(&p).copied().flatten();
            }
            Album {
                id: r.0,
                parent_id: parent,
                name: r.2,
                description: r.3,
                media_count: r.4,
                total_count: r.4,
                cover_media_id: r.5,
                date_from: r.6,
                date_to: r.7,
            }
        })
        .collect();
    roll_up(&mut list);
    Ok(list)
}

/// Parent albums count their sub-albums' media and borrow a cover and date
/// range from them when they hold no media themselves ("Bayramlar" → "Ramazan 2023").
fn roll_up(list: &mut [Album]) {
    let mut children: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, a) in list.iter().enumerate() {
        if let Some(p) = a.parent_id {
            children.entry(p).or_default().push(i);
        }
    }
    fn visit(i: usize, list: &mut [Album], children: &HashMap<i64, Vec<usize>>) {
        let kids = children.get(&list[i].id).cloned().unwrap_or_default();
        for k in kids {
            visit(k, list, children);
            let (total, cover, from, to) = (list[k].total_count, list[k].cover_media_id, list[k].date_from.clone(), list[k].date_to.clone());
            let a = &mut list[i];
            a.total_count += total;
            a.cover_media_id = a.cover_media_id.or(cover);
            a.date_from = match (a.date_from.take(), from) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            };
            a.date_to = match (a.date_to.take(), to) {
                (Some(x), Some(y)) => Some(x.max(y)),
                (x, y) => x.or(y),
            };
        }
    }
    let roots: Vec<usize> = list.iter().enumerate().filter(|(_, a)| a.parent_id.is_none()).map(|(i, _)| i).collect();
    for r in roots {
        visit(r, list, &children);
    }
}

pub async fn album_media(db: &Db, viewer: Viewer, album_id: i64, offset: i64, limit: i64) -> CoreResult<ClientPage> {
    if !albums(db, viewer).await?.iter().any(|a| a.id == album_id) {
        return Err(CoreError::NotFound);
    }
    let mut count = QueryBuilder::<Sqlite>::new(
        "SELECT COUNT(*) FROM media m JOIN media_categories mc ON mc.media_id = m.id WHERE mc.category_id = ",
    );
    count.push_bind(album_id);
    viewer.push_visible(&mut count, "m");
    let total: i64 = count.build_query_scalar().fetch_one(&db.pool).await?;

    let mut page = QueryBuilder::<Sqlite>::new(format!(
        "SELECT {ITEM_COLUMNS} FROM media m JOIN media_categories mc ON mc.media_id = m.id WHERE mc.category_id = "
    ));
    page.push_bind(album_id);
    viewer.push_visible(&mut page, "m");
    page.push(" ORDER BY m.taken_at IS NULL, m.taken_at, m.id LIMIT ")
        .push_bind(limit.clamp(1, 500))
        .push(" OFFSET ")
        .push_bind(offset.max(0));
    let items: Vec<ClientItem> = page.build_query_as().fetch_all(&db.pool).await?;
    Ok(ClientPage { total, offset, items })
}

#[derive(Debug, Serialize)]
pub struct Home {
    pub display_name: String,
    pub visible_media: i64,
    /// Top-level albums.
    pub albums: Vec<Album>,
    pub album_count: usize,
    pub years: Vec<(String, i64)>,
    /// Most recently added media.
    pub recent: Vec<ClientItem>,
}

pub async fn home(db: &Db, user: &User) -> CoreResult<Home> {
    let viewer = Viewer::of(user);
    let all = albums(db, viewer).await?;
    let album_count = all.len();
    let top = all.into_iter().filter(|a| a.parent_id.is_none()).collect();

    let mut years = QueryBuilder::<Sqlite>::new("SELECT substr(m.taken_at, 1, 4) AS y, COUNT(*) FROM media m WHERE m.taken_at IS NOT NULL");
    viewer.push_visible(&mut years, "m");
    years.push(" GROUP BY y ORDER BY y DESC");
    let years: Vec<(String, i64)> = years.build_query_as().fetch_all(&db.pool).await?;

    let mut recent = QueryBuilder::<Sqlite>::new(format!("SELECT {ITEM_COLUMNS} FROM media m WHERE m.thumb_ready = 1"));
    viewer.push_visible(&mut recent, "m");
    recent.push(" ORDER BY m.id DESC LIMIT 24");
    let recent: Vec<ClientItem> = recent.build_query_as().fetch_all(&db.pool).await?;

    Ok(Home {
        display_name: user.display_name.clone(),
        visible_media: years.iter().map(|(_, n)| n).sum::<i64>() + undated(db, viewer).await?,
        albums: top,
        album_count,
        years,
        recent,
    })
}

async fn undated(db: &Db, viewer: Viewer) -> CoreResult<i64> {
    let mut qb = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) FROM media m WHERE m.taken_at IS NULL");
    viewer.push_visible(&mut qb, "m");
    Ok(qb.build_query_scalar().fetch_one(&db.pool).await?)
}

#[derive(Debug, Serialize)]
pub struct MediaInfo {
    #[serde(flatten)]
    pub item: ClientItem,
    pub camera: Option<String>,
    pub tags: Vec<TagRef>,
    pub albums: Vec<(i64, String)>,
}

pub async fn media_info(db: &Db, user: &User, id: i64) -> CoreResult<MediaInfo> {
    if !access::can_view(db, user, id).await? {
        return Err(CoreError::NotFound);
    }
    let item: ClientItem = sqlx::query_as(&format!("SELECT {ITEM_COLUMNS} FROM media m WHERE m.id = ?"))
        .bind(id)
        .fetch_one(&db.pool)
        .await?;
    let camera: Option<String> = sqlx::query_scalar("SELECT camera FROM media WHERE id = ?").bind(id).fetch_one(&db.pool).await?;
    let tags = sqlx::query_as::<_, (i64, String, String, Option<String>)>(
        "SELECT t.id, t.name, tt.code, tt.color FROM media_tags mt
         JOIN tags t ON t.id = mt.tag_id JOIN tag_types tt ON tt.id = t.type_id
         WHERE mt.media_id = ? ORDER BY tt.sort_order, t.name COLLATE NOCASE",
    )
    .bind(id)
    .fetch_all(&db.pool)
    .await?
    .into_iter()
    .map(|(id, name, type_code, color)| TagRef { id, name, type_code, color })
    .collect();
    let visible: HashSet<i64> = albums(db, Viewer::of(user)).await?.iter().map(|a| a.id).collect();
    let albums = sqlx::query_as::<_, (i64, String)>(
        "SELECT c.id, c.name FROM media_categories mc JOIN categories c ON c.id = mc.category_id
         WHERE mc.media_id = ? ORDER BY c.name COLLATE NOCASE",
    )
    .bind(id)
    .fetch_all(&db.pool)
    .await?
    .into_iter()
    .filter(|(cid, _)| visible.contains(cid))
    .collect();
    Ok(MediaInfo { item, camera, tags, albums })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        catalog::{bulk, BulkOp},
        categories,
        users::{self, NewUser, Role},
    };

    #[tokio::test]
    async fn albums_follow_visibility() {
        let db = Db::open_memory().await.unwrap();
        for (id, hash) in [(1, "a"), (2, "b"), (3, "c")] {
            sqlx::query("INSERT INTO media (id, hash, kind, taken_at, thumb_ready) VALUES (?, ?, 'photo', '2022-01-01T00:00:00', 1)")
                .bind(id)
                .bind(hash)
                .execute(&db.pool)
                .await
                .unwrap();
        }
        let member = users::create(&db, NewUser { username: "uye".into(), display_name: "Üye".into(), password_hash: "x".into(), role: Role::Member })
            .await
            .unwrap();
        let v = Viewer::of(&member);

        let gizli = categories::create(&db, None, "Gizli Üst", None).await.unwrap();
        let koy = categories::create(&db, Some(gizli), "Köy", None).await.unwrap();
        bulk(&db, &[1, 2], &BulkOp::AddToCategory { category_id: koy }).await.unwrap();
        bulk(&db, &[3], &BulkOp::AddToCategory { category_id: gizli }).await.unwrap();
        assert!(albums(&db, v).await.unwrap().is_empty());

        categories::set_shares(&db, koy, &[1], false).await.unwrap();
        let list = albums(&db, v).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].name.as_str(), list[0].parent_id, list[0].media_count), ("Köy", None, 2), "private parent not shown");
        assert_eq!(album_media(&db, v, koy, 0, 10).await.unwrap().total, 2);
        assert!(matches!(album_media(&db, v, gizli, 0, 10).await, Err(CoreError::NotFound)));

        // A shared parent without own media shows its children's media and cover.
        let bayram = categories::create(&db, None, "Bayramlar", None).await.unwrap();
        let ramazan = categories::create(&db, Some(bayram), "Ramazan", None).await.unwrap();
        bulk(&db, &[1], &BulkOp::AddToCategory { category_id: ramazan }).await.unwrap();
        categories::set_shares(&db, bayram, &[1], true).await.unwrap();
        let list = albums(&db, v).await.unwrap();
        let b = list.iter().find(|a| a.id == bayram).unwrap();
        assert_eq!((b.media_count, b.total_count, b.cover_media_id), (0, 1, Some(1)));

        let h = home(&db, &member).await.unwrap();
        assert_eq!((h.visible_media, h.album_count, h.recent.len()), (2, 3, 2));
        assert!(matches!(media_info(&db, &member, 3).await, Err(CoreError::NotFound)));
        assert_eq!(media_info(&db, &member, 1).await.unwrap().albums, vec![(koy, "Köy".to_string()), (ramazan, "Ramazan".to_string())]);
    }
}
