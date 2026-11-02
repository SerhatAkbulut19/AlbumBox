//! First-run setup: creates the initial admin when no user exists.

use albumbox_core::{
    auth,
    users::{self, NewUser, Role},
};
use axum::{extract::State, http::HeaderMap, Json};
use axum_extra::extract::cookie::CookieJar;
use serde::{Deserialize, Serialize};

use super::session::{start_session, SessionResponse};
use crate::{
    auth::ClientIp,
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Serialize)]
pub struct SetupStatus {
    needs_setup: bool,
}

pub async fn status(State(st): State<AppState>) -> ApiResult<Json<SetupStatus>> {
    Ok(Json(SetupStatus { needs_setup: users::count(&st.db).await? == 0 }))
}

#[derive(Deserialize)]
pub struct SetupRequest {
    username: String,
    display_name: String,
    password: String,
}

pub async fn create_admin(
    State(st): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    jar: CookieJar,
    Json(req): Json<SetupRequest>,
) -> ApiResult<(CookieJar, Json<SessionResponse>)> {
    let _guard = st.setup_lock.lock().await;
    if users::count(&st.db).await? > 0 {
        return Err(ApiError::Conflict("already_setup"));
    }
    auth::validate_password(&req.password)?;
    let password = req.password;
    let password_hash = tokio::task::spawn_blocking(move || auth::hash_password(&password)).await??;
    let user = users::create(
        &st.db,
        NewUser { username: req.username, display_name: req.display_name, password_hash, role: Role::Admin },
    )
    .await?;
    tracing::info!(username = %user.username, "initial admin created");
    start_session(&st, user, true, &headers, ip, jar).await
}
