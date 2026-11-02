//! Request identity: session token from the `ab_session` cookie (web) or an
//! `Authorization: Bearer` header (Tauri), and the client IP.

use std::net::{IpAddr, SocketAddr};

use albumbox_core::{
    auth,
    users::{Role, User},
};
use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{header, request::Parts, HeaderMap},
};
use axum_extra::extract::cookie::CookieJar;

use crate::{error::ApiError, state::AppState};

pub const SESSION_COOKIE: &str = "ab_session";

pub fn token_from_headers(headers: &HeaderMap) -> Option<String> {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(|t| t.trim().to_string());
    bearer.or_else(|| CookieJar::from_headers(headers).get(SESSION_COOKIE).map(|c| c.value().to_string()))
}

/// Any signed-in, active user.
pub struct AuthUser {
    pub user: User,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = token_from_headers(&parts.headers).ok_or(ApiError::Unauthorized)?;
        let user = auth::session_user(&state.db, &token).await?.ok_or(ApiError::Unauthorized)?;
        Ok(AuthUser { user })
    }
}

/// Signed-in user with the admin role.
pub struct AdminUser(pub User);

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let AuthUser { user, .. } = AuthUser::from_request_parts(parts, state).await?;
        if user.role != Role::Admin {
            return Err(ApiError::Forbidden);
        }
        Ok(AdminUser(user))
    }
}

/// Peer IP when the server runs with connect info (absent in unit tests).
pub struct ClientIp(pub Option<IpAddr>);

impl<S: Send + Sync> FromRequestParts<S> for ClientIp {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(ClientIp(parts.extensions.get::<ConnectInfo<SocketAddr>>().map(|ci| ci.0.ip())))
    }
}
