//! Password hashing and session tokens.

use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::{
    users::{self, User},
    CoreError, CoreResult, Db,
};

pub const MIN_PASSWORD_LEN: usize = 6;

/// Argon2id hash. CPU-heavy: call from a blocking thread.
pub fn hash_password(password: &str) -> CoreResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| CoreError::Other(anyhow::anyhow!("hash: {e}")))
}

/// CPU-heavy: call from a blocking thread.
pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

/// Hash verified against for unknown usernames, so response time does not
/// reveal whether a username exists.
pub fn dummy_hash() -> &'static str {
    static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DUMMY.get_or_init(|| hash_password("albumbox-dummy-password").unwrap_or_default())
}

pub fn validate_password(password: &str) -> CoreResult<()> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(CoreError::Invalid("password_too_short"));
    }
    Ok(())
}

fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn iso(t: OffsetDateTime) -> String {
    t.format(&Rfc3339).unwrap_or_default()
}

fn now_iso() -> String {
    iso(OffsetDateTime::now_utc())
}

/// Creates a session and returns the raw token (only its hash is stored).
pub async fn create_session(
    db: &Db,
    user_id: i64,
    lifetime: Duration,
    user_agent: Option<&str>,
    ip: Option<&str>,
) -> CoreResult<String> {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let now = OffsetDateTime::now_utc();
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, created_at, expires_at, last_seen_at, user_agent, ip)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(token_hash(&token))
    .bind(user_id)
    .bind(iso(now))
    .bind(iso(now + lifetime))
    .bind(iso(now))
    .bind(user_agent)
    .bind(ip)
    .execute(&db.pool)
    .await?;
    sqlx::query("UPDATE users SET last_login_at = ? WHERE id = ?")
        .bind(iso(now))
        .bind(user_id)
        .execute(&db.pool)
        .await?;
    Ok(token)
}

/// Resolves a token to an active user. Expired sessions are removed.
pub async fn session_user(db: &Db, token: &str) -> CoreResult<Option<User>> {
    let hash = token_hash(token);
    let row: Option<(i64, String)> =
        sqlx::query_as("SELECT user_id, expires_at FROM sessions WHERE token_hash = ?")
            .bind(&hash)
            .fetch_optional(&db.pool)
            .await?;
    let Some((user_id, expires_at)) = row else { return Ok(None) };
    let expired = OffsetDateTime::parse(&expires_at, &Rfc3339)
        .map(|t| t <= OffsetDateTime::now_utc())
        .unwrap_or(true);
    if expired {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?").bind(&hash).execute(&db.pool).await?;
        return Ok(None);
    }
    let user = users::get(db, user_id).await?;
    Ok(user.filter(|u| u.is_active))
}

pub async fn delete_session(db: &Db, token: &str) -> CoreResult<()> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(token_hash(token))
        .execute(&db.pool)
        .await?;
    Ok(())
}

pub async fn delete_user_sessions(db: &Db, user_id: i64) -> CoreResult<()> {
    sqlx::query("DELETE FROM sessions WHERE user_id = ?").bind(user_id).execute(&db.pool).await?;
    Ok(())
}

pub async fn purge_expired_sessions(db: &Db) -> CoreResult<u64> {
    // RFC 3339 UTC strings with the same 'Z' suffix compare correctly as text.
    let r = sqlx::query("DELETE FROM sessions WHERE expires_at < ?").bind(now_iso()).execute(&db.pool).await?;
    Ok(r.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::users::{NewUser, Role};

    #[test]
    fn password_roundtrip() {
        let h = hash_password("gizli123").unwrap();
        assert!(verify_password("gizli123", &h));
        assert!(!verify_password("yanlis", &h));
        assert!(validate_password("12345").is_err());
    }

    #[tokio::test]
    async fn session_lifecycle() {
        let db = Db::open_memory().await.unwrap();
        let user = users::create(
            &db,
            NewUser {
                username: "ahmet".into(),
                display_name: "Ahmet".into(),
                password_hash: hash_password("123456").unwrap(),
                role: Role::Member,
            },
        )
        .await
        .unwrap();
        let token = create_session(&db, user.id, Duration::days(1), None, None).await.unwrap();
        assert_eq!(session_user(&db, &token).await.unwrap().unwrap().id, user.id);
        assert!(session_user(&db, "baska-token").await.unwrap().is_none());

        users::set_active(&db, user.id, false).await.unwrap();
        assert!(session_user(&db, &token).await.unwrap().is_none(), "inactive user must not resolve");

        users::set_active(&db, user.id, true).await.unwrap();
        delete_session(&db, &token).await.unwrap();
        assert!(session_user(&db, &token).await.unwrap().is_none());

        let expired = create_session(&db, user.id, Duration::seconds(-1), None, None).await.unwrap();
        assert!(session_user(&db, &expired).await.unwrap().is_none());
    }
}
