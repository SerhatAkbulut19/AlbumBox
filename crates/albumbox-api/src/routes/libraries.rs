//! Admin: libraries, scanning, folder suggestions and the server folder browser.

use albumbox_core::{
    jobs,
    libraries::{self, FolderSummary, Library},
    tags,
};
use albumbox_indexer::admin::{self, FsListing};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    auth::AdminUser,
    error::{ApiError, ApiResult},
    state::AppState,
};

pub async fn list(State(st): State<AppState>, _a: AdminUser) -> ApiResult<Json<Vec<Library>>> {
    Ok(Json(libraries::list(&st.db).await?))
}

#[derive(Deserialize)]
pub struct AddRequest {
    path: String,
    #[serde(default)]
    name: String,
    /// Start the first scan right away.
    #[serde(default = "yes")]
    scan: bool,
}

fn yes() -> bool {
    true
}

pub async fn add(
    State(st): State<AppState>,
    _a: AdminUser,
    Json(req): Json<AddRequest>,
) -> ApiResult<(StatusCode, Json<Library>)> {
    let lib = admin::add_library(&st.db, &req.path, &req.name).await?;
    tracing::info!(library = %lib.name, root = ?lib.last_seen_root, "library added");
    if req.scan {
        jobs::enqueue(&st.db, "scan", &json!({ "library_id": lib.id })).await?;
        st.engine.wake();
    }
    st.engine.refresh_libraries();
    Ok((StatusCode::CREATED, Json(lib)))
}

#[derive(Deserialize)]
pub struct RenameRequest {
    name: String,
}

pub async fn rename(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<String>,
    Json(req): Json<RenameRequest>,
) -> ApiResult<StatusCode> {
    libraries::rename(&st.db, &id, &req.name).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn scan(State(st): State<AppState>, _a: AdminUser, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    let lib = libraries::get(&st.db, &id).await?.ok_or(ApiError::NotFound)?;
    if !lib.is_online {
        return Err(ApiError::Conflict("library_offline"));
    }
    let job_id = jobs::enqueue(&st.db, "scan", &json!({ "library_id": lib.id })).await?;
    st.engine.wake();
    Ok(Json(json!({ "job_id": job_id })))
}

pub async fn refresh(State(st): State<AppState>, _a: AdminUser) -> StatusCode {
    st.engine.refresh_libraries();
    StatusCode::NO_CONTENT
}

pub async fn folders(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<FolderSummary>>> {
    libraries::get(&st.db, &id).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(libraries::folders(&st.db, &id).await?))
}

#[derive(Deserialize)]
pub struct TagFolderRequest {
    folder_rel: String,
    name: String,
    #[serde(default)]
    include_subfolders: bool,
}

/// Tags every media item in a folder with an event tag ("klasör adı önerisi").
pub async fn tag_folder(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<String>,
    Json(req): Json<TagFolderRequest>,
) -> ApiResult<Json<Value>> {
    libraries::get(&st.db, &id).await?.ok_or(ApiError::NotFound)?;
    let tag_id = tags::get_or_create(&st.db, "event", &req.name).await?;
    let tagged = tags::tag_folder(&st.db, tag_id, &id, &req.folder_rel, req.include_subfolders).await?;
    Ok(Json(json!({ "tag_id": tag_id, "tagged": tagged })))
}

#[derive(Deserialize)]
pub struct BrowseQuery {
    path: Option<String>,
}

pub async fn browse(_a: AdminUser, Query(q): Query<BrowseQuery>) -> ApiResult<Json<FsListing>> {
    Ok(Json(tokio::task::spawn_blocking(move || admin::browse(q.path.as_deref())).await??))
}
