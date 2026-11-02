//! Admin panel endpoints (admin role only).

use albumbox_core::{
    auth,
    stats::{self, AdminStats},
    users::{self, NewUser, Role, User},
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{
    auth::AdminUser,
    error::{ApiError, ApiResult},
    state::AppState,
};

pub async fn stats(State(st): State<AppState>, _admin: AdminUser) -> ApiResult<Json<AdminStats>> {
    Ok(Json(stats::admin_stats(&st.db).await?))
}

pub async fn list_users(State(st): State<AppState>, _admin: AdminUser) -> ApiResult<Json<Vec<User>>> {
    Ok(Json(users::list(&st.db).await?))
}

#[derive(Deserialize)]
pub struct CreateUserRequest {
    username: String,
    display_name: String,
    password: String,
    role: Role,
}

pub async fn create_user(
    State(st): State<AppState>,
    _admin: AdminUser,
    Json(req): Json<CreateUserRequest>,
) -> ApiResult<(StatusCode, Json<User>)> {
    users::validate_username(req.username.trim())?;
    auth::validate_password(&req.password)?;
    let password = req.password;
    let password_hash = tokio::task::spawn_blocking(move || auth::hash_password(&password)).await??;
    let user = users::create(
        &st.db,
        NewUser { username: req.username, display_name: req.display_name, password_hash, role: req.role },
    )
    .await?;
    Ok((StatusCode::CREATED, Json(user)))
}

#[derive(Deserialize)]
pub struct UpdateUserRequest {
    display_name: String,
    role: Role,
    is_active: bool,
}

pub async fn update_user(
    State(st): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
    Json(req): Json<UpdateUserRequest>,
) -> ApiResult<Json<User>> {
    let current = users::get(&st.db, id).await?.ok_or(ApiError::NotFound)?;
    if id == admin.id && (!req.is_active || req.role != Role::Admin) {
        return Err(ApiError::BadRequest("cannot_demote_self"));
    }
    let loses_admin = current.role == Role::Admin && current.is_active && (req.role != Role::Admin || !req.is_active);
    if loses_admin && users::other_active_admins(&st.db, id).await? == 0 {
        return Err(ApiError::BadRequest("last_admin"));
    }

    let user = users::update(&st.db, id, &req.display_name, req.role).await?;
    if current.is_active != req.is_active {
        users::set_active(&st.db, id, req.is_active).await?;
        if !req.is_active {
            auth::delete_user_sessions(&st.db, id).await?;
        }
    }
    Ok(Json(User { is_active: req.is_active, ..user }))
}

#[derive(Deserialize)]
pub struct ResetPasswordRequest {
    password: String,
}

pub async fn reset_password(
    State(st): State<AppState>,
    AdminUser(admin): AdminUser,
    Path(id): Path<i64>,
    Json(req): Json<ResetPasswordRequest>,
) -> ApiResult<StatusCode> {
    auth::validate_password(&req.password)?;
    let password = req.password;
    let hash = tokio::task::spawn_blocking(move || auth::hash_password(&password)).await??;
    users::set_password_hash(&st.db, id, &hash).await?;
    // Sign the user out everywhere, except the admin resetting their own password.
    if id != admin.id {
        auth::delete_user_sessions(&st.db, id).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}
