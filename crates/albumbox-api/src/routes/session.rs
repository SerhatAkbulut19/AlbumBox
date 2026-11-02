//! Login, logout and current user.

use std::net::IpAddr;

use albumbox_core::{
    auth,
    users::{self, User},
};
use axum::{
    extract::State,
    http::{header, HeaderMap},
    Json,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use serde::{Deserialize, Serialize};

use crate::{
    auth::{token_from_headers, AuthUser, ClientIp, SESSION_COOKIE},
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Serialize)]
pub struct SessionResponse {
    user: User,
    /// Bearer token for the desktop app; the web UI relies on the HttpOnly cookie.
    token: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
    #[serde(default)]
    remember: bool,
}

pub async fn login(
    State(st): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> ApiResult<(CookieJar, Json<SessionResponse>)> {
    if st.login_limiter.is_blocked(ip) {
        return Err(ApiError::TooManyRequests);
    }
    let found = users::find_for_login(&st.db, &req.username).await?;
    let password = req.password;
    let (user, valid) = tokio::task::spawn_blocking(move || match found {
        Some((user, hash)) => {
            let ok = auth::verify_password(&password, &hash);
            (Some(user), ok)
        }
        None => {
            auth::verify_password(&password, auth::dummy_hash());
            (None, false)
        }
    })
    .await?;

    match user {
        Some(user) if valid && user.is_active => {
            st.login_limiter.record_success(ip);
            tracing::info!(username = %user.username, ?ip, "login");
            start_session(&st, user, req.remember, &headers, ip, jar).await
        }
        _ => {
            st.login_limiter.record_failure(ip);
            tracing::warn!(username = %req.username, ?ip, "failed login");
            Err(ApiError::BadRequest("invalid_credentials"))
        }
    }
}

pub async fn logout(State(st): State<AppState>, headers: HeaderMap, jar: CookieJar) -> ApiResult<CookieJar> {
    if let Some(token) = token_from_headers(&headers) {
        auth::delete_session(&st.db, &token).await?;
    }
    Ok(jar.remove(Cookie::build(SESSION_COOKIE).path("/")))
}

#[derive(Serialize)]
pub struct MeResponse {
    user: User,
    profile_ids: Vec<i64>,
}

pub async fn me(State(st): State<AppState>, AuthUser { user, .. }: AuthUser) -> ApiResult<Json<MeResponse>> {
    let profile_ids = users::profile_ids(&st.db, user.id).await?;
    Ok(Json(MeResponse { user, profile_ids }))
}

/// Creates a session and sets the cookie. "Beni hatırla" gives a persistent
/// cookie; otherwise the cookie ends with the browser session.
pub async fn start_session(
    st: &AppState,
    user: User,
    remember: bool,
    headers: &HeaderMap,
    ip: Option<IpAddr>,
    jar: CookieJar,
) -> ApiResult<(CookieJar, Json<SessionResponse>)> {
    let lifetime = if remember {
        time::Duration::days(st.config.remember_days)
    } else {
        time::Duration::hours(st.config.session_hours)
    };
    let user_agent = headers.get(header::USER_AGENT).and_then(|v| v.to_str().ok());
    let ip_text = ip.map(|i| i.to_string());
    let token = auth::create_session(&st.db, user.id, lifetime, user_agent, ip_text.as_deref()).await?;

    let mut cookie = Cookie::build((SESSION_COOKIE, token.clone())).path("/").http_only(true).same_site(SameSite::Lax);
    if remember {
        cookie = cookie.max_age(lifetime);
    }
    Ok((jar.add(cookie), Json(SessionResponse { user, token })))
}
