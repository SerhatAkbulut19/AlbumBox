//! Profiles: publishing audiences ("Tüm Aile", "Yakın Aile", "Çocuklar" …).
//! Membership changes never touch `media_access`: access is stored per profile.

use serde::Serialize;

use crate::{users::ALL_FAMILY_PROFILE_ID, CoreError, CoreResult, Db};

#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub is_builtin: bool,
    pub member_ids: Vec<i64>,
    /// Media published to this profile.
    pub visible_media: i64,
}

pub async fn list(db: &Db) -> CoreResult<Vec<Profile>> {
    let rows: Vec<(i64, String, Option<String>, bool, Option<String>, i64)> = sqlx::query_as(
        "SELECT p.id, p.name, p.description, p.is_builtin,
                (SELECT group_concat(user_id) FROM user_profiles up WHERE up.profile_id = p.id),
                (SELECT COUNT(*) FROM media_access ma WHERE ma.profile_id = p.id)
         FROM profiles p
         ORDER BY p.is_builtin DESC, p.name COLLATE NOCASE",
    )
    .fetch_all(&db.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, name, description, is_builtin, members, visible_media)| Profile {
            id,
            name,
            description,
            is_builtin,
            member_ids: members.unwrap_or_default().split(',').filter_map(|s| s.parse().ok()).collect(),
            visible_media,
        })
        .collect())
}

fn clean(name: &str) -> CoreResult<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() { Err(CoreError::Invalid("name_required")) } else { Ok(name) }
}

fn map_unique(e: sqlx::Error) -> CoreError {
    match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => CoreError::Conflict("profile_exists"),
        _ => CoreError::Db(e),
    }
}

pub async fn create(db: &Db, name: &str, description: Option<&str>) -> CoreResult<i64> {
    sqlx::query_scalar("INSERT INTO profiles (name, description) VALUES (?, ?) RETURNING id")
        .bind(clean(name)?)
        .bind(description.map(str::trim).filter(|d| !d.is_empty()))
        .fetch_one(&db.pool)
        .await
        .map_err(map_unique)
}

pub async fn update(db: &Db, id: i64, name: &str, description: Option<&str>) -> CoreResult<()> {
    let r = sqlx::query("UPDATE profiles SET name = ?, description = ? WHERE id = ?")
        .bind(clean(name)?)
        .bind(description.map(str::trim).filter(|d| !d.is_empty()))
        .bind(id)
        .execute(&db.pool)
        .await
        .map_err(map_unique)?;
    if r.rows_affected() == 0 { Err(CoreError::NotFound) } else { Ok(()) }
}

pub async fn delete(db: &Db, id: i64) -> CoreResult<()> {
    if id == ALL_FAMILY_PROFILE_ID {
        return Err(CoreError::Invalid("builtin_profile"));
    }
    // Shares and access rows go with it (ON DELETE CASCADE).
    let r = sqlx::query("DELETE FROM profiles WHERE id = ? AND is_builtin = 0").bind(id).execute(&db.pool).await?;
    if r.rows_affected() == 0 { Err(CoreError::NotFound) } else { Ok(()) }
}

/// Replaces the members of a profile. The built-in profile always contains everyone.
pub async fn set_members(db: &Db, id: i64, user_ids: &[i64]) -> CoreResult<()> {
    if id == ALL_FAMILY_PROFILE_ID {
        return Err(CoreError::Invalid("builtin_profile"));
    }
    let mut tx = db.pool.begin().await?;
    let found: Option<i64> = sqlx::query_scalar("SELECT id FROM profiles WHERE id = ?").bind(id).fetch_optional(&mut *tx).await?;
    found.ok_or(CoreError::NotFound)?;
    sqlx::query("DELETE FROM user_profiles WHERE profile_id = ?").bind(id).execute(&mut *tx).await?;
    for u in user_ids {
        sqlx::query("INSERT OR IGNORE INTO user_profiles (user_id, profile_id) SELECT id, ? FROM users WHERE id = ?")
            .bind(id)
            .bind(u)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::{self, NewUser, Role};

    #[tokio::test]
    async fn members_and_builtin_rules() {
        let db = Db::open_memory().await.unwrap();
        let u = users::create(&db, NewUser { username: "ali".into(), display_name: "Ali".into(), password_hash: "x".into(), role: Role::Member })
            .await
            .unwrap();
        let yakin = create(&db, "Yakın Aile", None).await.unwrap();
        assert!(matches!(create(&db, "yakın aile", None).await, Err(CoreError::Conflict(_))));
        set_members(&db, yakin, &[u.id, 9999]).await.unwrap();
        let all = list(&db).await.unwrap();
        assert_eq!(all[0].id, ALL_FAMILY_PROFILE_ID);
        assert_eq!(all.iter().find(|p| p.id == yakin).unwrap().member_ids, vec![u.id]);
        assert!(delete(&db, ALL_FAMILY_PROFILE_ID).await.is_err());
        assert!(set_members(&db, ALL_FAMILY_PROFILE_ID, &[]).await.is_err());
        delete(&db, yakin).await.unwrap();
        assert_eq!(users::profile_ids(&db, u.id).await.unwrap(), vec![ALL_FAMILY_PROFILE_ID]);
    }
}
