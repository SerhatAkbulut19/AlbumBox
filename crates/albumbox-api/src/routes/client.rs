//! Client endpoints. Every query is limited to what the signed-in identity may see.

use albumbox_core::{
    client::{self, Album, Home, MediaInfo},
    search::{self, ClientPage, SearchQuery, SearchResult, Viewer},
};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::{auth::AuthUser, error::ApiResult, state::AppState};

pub async fn home(State(st): State<AppState>, AuthUser { user }: AuthUser) -> ApiResult<Json<Home>> {
    Ok(Json(client::home(&st.db, &user).await?))
}

pub async fn search(
    State(st): State<AppState>,
    AuthUser { user }: AuthUser,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<SearchResult>> {
    Ok(Json(search::search(&st.db, Viewer::of(&user), &q).await?))
}

#[derive(Deserialize)]
pub struct GroupPaging {
    /// Group key from the search result; omit for all results.
    key: Option<String>,
    #[serde(default)]
    offset: i64,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    120
}

pub async fn search_media(
    State(st): State<AppState>,
    AuthUser { user }: AuthUser,
    Query(p): Query<GroupPaging>,
    Query(q): Query<SearchQuery>,
) -> ApiResult<Json<ClientPage>> {
    Ok(Json(search::group_media(&st.db, Viewer::of(&user), &q, p.key.as_deref(), p.offset, p.limit).await?))
}

pub async fn albums(State(st): State<AppState>, AuthUser { user }: AuthUser) -> ApiResult<Json<Vec<Album>>> {
    Ok(Json(client::albums(&st.db, Viewer::of(&user)).await?))
}

#[derive(Deserialize)]
pub struct Paging {
    #[serde(default)]
    offset: i64,
    #[serde(default = "default_limit")]
    limit: i64,
}

pub async fn album_media(
    State(st): State<AppState>,
    AuthUser { user }: AuthUser,
    Path(id): Path<i64>,
    Query(p): Query<Paging>,
) -> ApiResult<Json<ClientPage>> {
    Ok(Json(client::album_media(&st.db, Viewer::of(&user), id, p.offset, p.limit).await?))
}

pub async fn media_info(State(st): State<AppState>, AuthUser { user }: AuthUser, Path(id): Path<i64>) -> ApiResult<Json<MediaInfo>> {
    Ok(Json(client::media_info(&st.db, &user, id).await?))
}
