//! Admin management: media catalogue, categories, tags, tag types, profiles.

use albumbox_core::{
    catalog::{self, BulkOp, MediaDetail, MediaFilter, MediaPage},
    categories::{self, Category},
    profiles::{self, Profile},
    tags::{self, Tag, TagType},
};
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

// ---------------------------------------------------------------------------
// Media
// ---------------------------------------------------------------------------

// Paging and filter are read from the same query string by two extractors:
// `serde(flatten)` would break numeric fields with urlencoded input.
#[derive(Deserialize)]
pub struct Paging {
    #[serde(default)]
    offset: i64,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    200
}

pub async fn media_list(
    State(st): State<AppState>,
    _a: AdminUser,
    Query(p): Query<Paging>,
    Query(f): Query<MediaFilter>,
) -> ApiResult<Json<MediaPage>> {
    Ok(Json(catalog::list(&st.db, &f, p.offset, p.limit).await?))
}

pub async fn media_ids(State(st): State<AppState>, _a: AdminUser, Query(f): Query<MediaFilter>) -> ApiResult<Json<Vec<i64>>> {
    Ok(Json(catalog::ids(&st.db, &f, catalog::MAX_BULK as i64).await?))
}

pub async fn media_detail(State(st): State<AppState>, _a: AdminUser, Path(id): Path<i64>) -> ApiResult<Json<MediaDetail>> {
    Ok(Json(catalog::detail(&st.db, id).await?.ok_or(ApiError::NotFound)?))
}

#[derive(Deserialize)]
pub struct BulkRequest {
    ids: Vec<i64>,
    #[serde(flatten)]
    op: BulkOp,
}

pub async fn media_bulk(State(st): State<AppState>, _a: AdminUser, Json(req): Json<BulkRequest>) -> ApiResult<Json<Value>> {
    let changed = catalog::bulk(&st.db, &req.ids, &req.op).await?;
    Ok(Json(json!({ "changed": changed })))
}

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

pub async fn category_list(State(st): State<AppState>, _a: AdminUser) -> ApiResult<Json<Vec<Category>>> {
    Ok(Json(categories::list(&st.db).await?))
}

#[derive(Deserialize)]
pub struct CategoryBody {
    #[serde(default)]
    parent_id: Option<i64>,
    name: String,
    #[serde(default)]
    description: Option<String>,
}

pub async fn category_create(
    State(st): State<AppState>,
    _a: AdminUser,
    Json(b): Json<CategoryBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let id = categories::create(&st.db, b.parent_id, &b.name, b.description.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn category_update(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<i64>,
    Json(b): Json<CategoryBody>,
) -> ApiResult<StatusCode> {
    categories::update(&st.db, id, &b.name, b.description.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn category_delete(State(st): State<AppState>, _a: AdminUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    categories::delete(&st.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct MoveBody {
    parent_id: Option<i64>,
    #[serde(default)]
    sort_order: Option<i64>,
}

pub async fn category_move(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<i64>,
    Json(b): Json<MoveBody>,
) -> ApiResult<StatusCode> {
    categories::move_to(&st.db, id, b.parent_id, b.sort_order).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct SharesBody {
    profile_ids: Vec<i64>,
    #[serde(default)]
    include_children: bool,
}

pub async fn category_shares(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<i64>,
    Json(b): Json<SharesBody>,
) -> ApiResult<StatusCode> {
    categories::set_shares(&st.db, id, &b.profile_ids, b.include_children).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct CoverBody {
    media_id: Option<i64>,
}

pub async fn category_cover(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<i64>,
    Json(b): Json<CoverBody>,
) -> ApiResult<StatusCode> {
    categories::set_cover(&st.db, id, b.media_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Tag types and tags
// ---------------------------------------------------------------------------

pub async fn type_list(State(st): State<AppState>, _a: AdminUser) -> ApiResult<Json<Vec<TagType>>> {
    Ok(Json(tags::list_types(&st.db).await?))
}

#[derive(Deserialize)]
pub struct TypeBody {
    name: String,
    #[serde(default)]
    color: Option<String>,
}

pub async fn type_create(
    State(st): State<AppState>,
    _a: AdminUser,
    Json(b): Json<TypeBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let id = tags::create_type(&st.db, &b.name, b.color.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn type_update(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<i64>,
    Json(b): Json<TypeBody>,
) -> ApiResult<StatusCode> {
    tags::update_type(&st.db, id, &b.name, b.color.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn type_delete(State(st): State<AppState>, _a: AdminUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    tags::delete_type(&st.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct TagListQuery {
    #[serde(rename = "type")]
    type_id: Option<i64>,
    q: Option<String>,
    #[serde(default = "default_suggest")]
    limit: i64,
}

fn default_suggest() -> i64 {
    12
}

/// `?q=` gives autocomplete suggestions, otherwise the full list (optionally per type).
pub async fn tag_list(State(st): State<AppState>, _a: AdminUser, Query(q): Query<TagListQuery>) -> ApiResult<Json<Vec<Tag>>> {
    Ok(Json(match q.q.as_deref().filter(|s| !s.trim().is_empty()) {
        Some(text) => tags::suggest(&st.db, text, q.type_id, q.limit.clamp(1, 50)).await?,
        None => tags::list(&st.db, q.type_id).await?,
    }))
}

#[derive(Deserialize)]
pub struct TagCreateBody {
    type_id: i64,
    name: String,
    #[serde(default)]
    parent_id: Option<i64>,
}

/// Returns the existing tag when one with the same (normalized) name exists.
pub async fn tag_create(State(st): State<AppState>, _a: AdminUser, Json(b): Json<TagCreateBody>) -> ApiResult<Json<Tag>> {
    let id = tags::get_or_create_in(&st.db, b.type_id, &b.name).await?;
    if b.parent_id.is_some() {
        tags::set_parent(&st.db, id, b.parent_id).await?;
    }
    Ok(Json(tags::get(&st.db, id).await?.ok_or(ApiError::NotFound)?))
}

#[derive(Deserialize)]
pub struct TagUpdateBody {
    name: String,
    #[serde(default)]
    parent_id: Option<i64>,
}

pub async fn tag_update(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<i64>,
    Json(b): Json<TagUpdateBody>,
) -> ApiResult<StatusCode> {
    tags::rename(&st.db, id, &b.name).await?;
    tags::set_parent(&st.db, id, b.parent_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct MergeBody {
    target_id: i64,
}

pub async fn tag_merge(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<i64>,
    Json(b): Json<MergeBody>,
) -> ApiResult<StatusCode> {
    tags::merge(&st.db, id, b.target_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn tag_delete(State(st): State<AppState>, _a: AdminUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    tags::delete(&st.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Profiles
// ---------------------------------------------------------------------------

pub async fn profile_list(State(st): State<AppState>, _a: AdminUser) -> ApiResult<Json<Vec<Profile>>> {
    Ok(Json(profiles::list(&st.db).await?))
}

#[derive(Deserialize)]
pub struct ProfileBody {
    name: String,
    #[serde(default)]
    description: Option<String>,
}

pub async fn profile_create(
    State(st): State<AppState>,
    _a: AdminUser,
    Json(b): Json<ProfileBody>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let id = profiles::create(&st.db, &b.name, b.description.as_deref()).await?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn profile_update(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<i64>,
    Json(b): Json<ProfileBody>,
) -> ApiResult<StatusCode> {
    profiles::update(&st.db, id, &b.name, b.description.as_deref()).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn profile_delete(State(st): State<AppState>, _a: AdminUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    profiles::delete(&st.db, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct MembersBody {
    user_ids: Vec<i64>,
}

pub async fn profile_members(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(id): Path<i64>,
    Json(b): Json<MembersBody>,
) -> ApiResult<StatusCode> {
    profiles::set_members(&st.db, id, &b.user_ids).await?;
    Ok(StatusCode::NO_CONTENT)
}
