//! Media files for signed-in users. Every request checks visibility first;
//! media the user may not see answers 404, exactly like media that does not exist.

use albumbox_core::{access, media};
use albumbox_indexer::thumbs;
use axum::{
    body::Body,
    extract::{Path, Request, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use tower::ServiceExt;
use tower_http::services::ServeFile;

use crate::{
    auth::AuthUser,
    error::{ApiError, ApiResult},
    state::AppState,
};

async fn ensure_visible(st: &AppState, user: &albumbox_core::users::User, id: i64) -> ApiResult<()> {
    if access::can_view(&st.db, user, id).await? { Ok(()) } else { Err(ApiError::NotFound) }
}

/// `GET /media/{id}/thumb/{size}` with size 256 or 1024. Works while the disk is unplugged.
pub async fn thumb(
    State(st): State<AppState>,
    AuthUser { user }: AuthUser,
    Path((id, size)): Path<(i64, u32)>,
) -> ApiResult<Response> {
    if !thumbs::SIZES.contains(&size) {
        return Err(ApiError::NotFound);
    }
    ensure_visible(&st, &user, id).await?;
    let hash = media::hash_of(&st.db, id).await?.ok_or(ApiError::NotFound)?;
    let path = thumbs::thumb_path(&st.engine.cache_dir, &hash, size);
    let bytes = tokio::fs::read(&path).await.map_err(|_| ApiError::NotFound)?;
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg")),
            // The id → content mapping never changes, so browsers may keep it.
            (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=2592000, immutable")),
        ],
        Body::from(bytes),
    )
        .into_response())
}

/// `GET /media/{id}/original`: the file on the library disk, with Range support for video seeking.
pub async fn original(
    State(st): State<AppState>,
    AuthUser { user }: AuthUser,
    Path(id): Path<i64>,
    req: Request,
) -> ApiResult<Response> {
    ensure_visible(&st, &user, id).await?;
    let (root, rel, _kind) = media::original_location(&st.db, id).await?.ok_or(ApiError::Conflict("disk_offline"))?;
    let path = rel.split('\\').fold(std::path::PathBuf::from(root), |p, s| p.join(s));
    let mut res = ServeFile::new(&path).oneshot(req).await.map_err(|e| ApiError::Internal(e.into()))?.into_response();
    if res.status() == StatusCode::NOT_FOUND {
        // Disk unplugged since the last check.
        st.engine.refresh_libraries();
        return Err(ApiError::Conflict("disk_offline"));
    }
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=3600"));
    Ok(res)
}
