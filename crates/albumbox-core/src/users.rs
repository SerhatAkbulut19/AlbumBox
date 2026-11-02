//! User accounts.

use serde::{Deserialize, Serialize};

use crate::{CoreError, CoreResult, Db};

/// Built-in profile every user belongs to.
pub const ALL_FAMILY_PROFILE_ID: i64 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Member,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Member => "member",
        }
    }

    fn parse(s: &str) -> Role {
        if s == "admin" { Role::Admin } else { Role::Member }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: i64,
    pub username: String,
    pub display_name: String,
    pub role: Role,
    pub is_active: bool,
    pub created_at: String,
    pub last_login_at: Option<String>,
}

pub struct NewUser {
    pub username: String,
    pub display_name: String,
    pub password_hash: String,
    pub role: Role,
}

type UserRow = (i64, String, String, String, bool, String, Option<String>);

const USER_COLUMNS: &str = "id, username, display_name, role, is_active, created_at, last_login_at";

fn from_row((id, username, display_name, role, is_active, created_at, last_login_at): UserRow) -> User {
    User { id, username, display_name, role: Role::parse(&role), is_active, created_at, last_login_at }
}

pub fn validate_username(username: &str) -> CoreResult<()> {
    let ok = (3..=32).contains(&username.chars().count())
        && username.chars().all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if ok { Ok(()) } else { Err(CoreError::Invalid("invalid_username")) }
}

pub async fn count(db: &Db) -> CoreResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&db.pool).await?)
}

pub async fn get(db: &Db, id: i64) -> CoreResult<Option<User>> {
    let row: Option<UserRow> = sqlx::query_as(&format!("SELECT {USER_COLUMNS} FROM users WHERE id = ?"))
        .bind(id)
        .fetch_optional(&db.pool)
        .await?;
    Ok(row.map(from_row))
}

/// Returns the user and its password hash for login.
pub async fn find_for_login(db: &Db, username: &str) -> CoreResult<Option<(User, String)>> {
    let row: Option<(i64, String, String, String, bool, String, Option<String>, String)> =
        sqlx::query_as(&format!("SELECT {USER_COLUMNS}, password_hash FROM users WHERE username = ?"))
            .bind(username.trim())
            .fetch_optional(&db.pool)
            .await?;
    Ok(row.map(|(a, b, c, d, e, f, g, hash)| (from_row((a, b, c, d, e, f, g)), hash)))
}

pub async fn list(db: &Db) -> CoreResult<Vec<User>> {
    let rows: Vec<UserRow> = sqlx::query_as(&format!(
        "SELECT {USER_COLUMNS} FROM users ORDER BY role, display_name COLLATE NOCASE"
    ))
    .fetch_all(&db.pool)
    .await?;
    Ok(rows.into_iter().map(from_row).collect())
}

/// Creates a user and adds it to the built-in "Tüm Aile" profile.
pub async fn create(db: &Db, new: NewUser) -> CoreResult<User> {
    validate_username(new.username.trim())?;
    if new.display_name.trim().is_empty() {
        return Err(CoreError::Invalid("display_name_required"));
    }
    let mut tx = db.pool.begin().await?;
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO users (username, display_name, password_hash, role) VALUES (?, ?, ?, ?) RETURNING id",
    )
    .bind(new.username.trim())
    .bind(new.display_name.trim())
    .bind(&new.password_hash)
    .bind(new.role.as_str())
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(d) if d.is_unique_violation() => CoreError::Conflict("username_taken"),
        _ => CoreError::Db(e),
    })?;
    sqlx::query("INSERT INTO user_profiles (user_id, profile_id) VALUES (?, ?)")
        .bind(id)
        .bind(ALL_FAMILY_PROFILE_ID)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    get(db, id).await?.ok_or(CoreError::NotFound)
}

pub async fn update(db: &Db, id: i64, display_name: &str, role: Role) -> CoreResult<User> {
    if display_name.trim().is_empty() {
        return Err(CoreError::Invalid("display_name_required"));
    }
    let r = sqlx::query("UPDATE users SET display_name = ?, role = ? WHERE id = ?")
        .bind(display_name.trim())
        .bind(role.as_str())
        .bind(id)
        .execute(&db.pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(CoreError::NotFound);
    }
    get(db, id).await?.ok_or(CoreError::NotFound)
}

pub async fn set_active(db: &Db, id: i64, active: bool) -> CoreResult<()> {
    let r = sqlx::query("UPDATE users SET is_active = ? WHERE id = ?")
        .bind(active)
        .bind(id)
        .execute(&db.pool)
        .await?;
    if r.rows_affected() == 0 { Err(CoreError::NotFound) } else { Ok(()) }
}

pub async fn set_password_hash(db: &Db, id: i64, hash: &str) -> CoreResult<()> {
    let r = sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
        .bind(hash)
        .bind(id)
        .execute(&db.pool)
        .await?;
    if r.rows_affected() == 0 { Err(CoreError::NotFound) } else { Ok(()) }
}

/// Number of active admins other than `except_id`; used to always keep one admin.
pub async fn other_active_admins(db: &Db, except_id: i64) -> CoreResult<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE role = 'admin' AND is_active = 1 AND id <> ?")
        .bind(except_id)
        .fetch_one(&db.pool)
        .await?)
}

/// Profile ids the user belongs to; used for visibility filtering.
pub async fn profile_ids(db: &Db, user_id: i64) -> CoreResult<Vec<i64>> {
    Ok(sqlx::query_scalar("SELECT profile_id FROM user_profiles WHERE user_id = ? ORDER BY profile_id")
        .bind(user_id)
        .fetch_all(&db.pool)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_user(name: &str, role: Role) -> NewUser {
        NewUser { username: name.into(), display_name: name.into(), password_hash: "x".into(), role }
    }

    #[tokio::test]
    async fn create_adds_all_family_profile_and_rejects_duplicates() {
        let db = Db::open_memory().await.unwrap();
        let u = create(&db, new_user("Mehmet", Role::Member)).await.unwrap();
        assert_eq!(profile_ids(&db, u.id).await.unwrap(), vec![ALL_FAMILY_PROFILE_ID]);
        // Usernames are case-insensitive.
        assert!(matches!(create(&db, new_user("mehmet", Role::Member)).await, Err(CoreError::Conflict(_))));
        assert!(find_for_login(&db, "MEHMET").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn keeps_track_of_other_admins() {
        let db = Db::open_memory().await.unwrap();
        let a = create(&db, new_user("admin1", Role::Admin)).await.unwrap();
        assert_eq!(other_active_admins(&db, a.id).await.unwrap(), 0);
        create(&db, new_user("admin2", Role::Admin)).await.unwrap();
        assert_eq!(other_active_admins(&db, a.id).await.unwrap(), 1);
    }

    #[test]
    fn username_rules() {
        assert!(validate_username("serhat.a").is_ok());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("ad soyad").is_err());
    }
}
