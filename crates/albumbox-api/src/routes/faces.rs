//! Admin: face recognition (Kişiler page).

use albumbox_core::{
    faces::{self, Cluster, FaceRef, FaceStats, MediaFace, PersonSummary},
    jobs,
};
use albumbox_indexer::faces as engine_faces;
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    auth::AdminUser,
    error::{ApiError, ApiResult},
    state::AppState,
};

#[derive(Serialize)]
pub struct Status {
    available: bool,
    models_dir: String,
    #[serde(flatten)]
    stats: FaceStats,
}

pub async fn status(State(st): State<AppState>, _a: AdminUser) -> ApiResult<Json<Status>> {
    Ok(Json(Status {
        available: st.engine.snapshot().faces_available,
        models_dir: st.config.models_dir.display().to_string(),
        stats: faces::stats(&st.db).await?,
    }))
}

pub async fn scan(State(st): State<AppState>, _a: AdminUser) -> ApiResult<Json<Value>> {
    if !st.engine.snapshot().faces_available {
        return Err(ApiError::Conflict("face_models_missing"));
    }
    let job_id = jobs::enqueue(&st.db, "faces", &json!({})).await?;
    st.engine.wake();
    Ok(Json(json!({ "job_id": job_id })))
}

#[derive(Deserialize)]
pub struct ClusterQuery {
    #[serde(default = "two")]
    min: i64,
}

fn two() -> i64 {
    2
}

pub async fn clusters(State(st): State<AppState>, _a: AdminUser, Query(q): Query<ClusterQuery>) -> ApiResult<Json<Vec<Cluster>>> {
    Ok(Json(faces::clusters(&st.db, q.min.max(1)).await?))
}

pub async fn persons(State(st): State<AppState>, _a: AdminUser) -> ApiResult<Json<Vec<PersonSummary>>> {
    Ok(Json(faces::persons(&st.db).await?))
}

#[derive(Deserialize)]
pub struct PersonFacesQuery {
    #[serde(default = "confirmed")]
    status: String,
}

fn confirmed() -> String {
    "confirmed".into()
}

pub async fn person_faces(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(tag_id): Path<i64>,
    Query(q): Query<PersonFacesQuery>,
) -> ApiResult<Json<Vec<FaceRef>>> {
    let status = if q.status == "suggested" { "suggested" } else { "confirmed" };
    Ok(Json(faces::person_faces(&st.db, tag_id, status, 500).await?))
}

#[derive(Deserialize)]
pub struct NameBody {
    tag_id: i64,
}

/// "Bu grup Ahmet": every face in the group is confirmed as the person.
pub async fn name_cluster(
    State(st): State<AppState>,
    _a: AdminUser,
    Path(cluster_id): Path<i64>,
    Json(b): Json<NameBody>,
) -> ApiResult<Json<Value>> {
    let ids = faces::cluster_face_ids(&st.db, cluster_id).await?;
    if ids.is_empty() {
        return Err(ApiError::NotFound);
    }
    let changed = faces::confirm(&st.db, &ids, b.tag_id).await?;
    st.engine.request_regroup();
    Ok(Json(json!({ "changed": changed })))
}

pub async fn ignore_cluster(State(st): State<AppState>, _a: AdminUser, Path(cluster_id): Path<i64>) -> ApiResult<Json<Value>> {
    let ids = faces::cluster_face_ids(&st.db, cluster_id).await?;
    let changed = faces::reject(&st.db, &ids).await?;
    Ok(Json(json!({ "changed": changed })))
}

#[derive(Deserialize)]
pub struct ConfirmBody {
    face_ids: Vec<i64>,
    tag_id: i64,
}

pub async fn confirm(State(st): State<AppState>, _a: AdminUser, Json(b): Json<ConfirmBody>) -> ApiResult<Json<Value>> {
    let changed = faces::confirm(&st.db, &b.face_ids, b.tag_id).await?;
    st.engine.request_regroup();
    Ok(Json(json!({ "changed": changed })))
}

#[derive(Deserialize)]
pub struct RejectBody {
    face_ids: Vec<i64>,
}

pub async fn reject(State(st): State<AppState>, _a: AdminUser, Json(b): Json<RejectBody>) -> ApiResult<Json<Value>> {
    let changed = faces::reject(&st.db, &b.face_ids).await?;
    st.engine.request_regroup();
    Ok(Json(json!({ "changed": changed })))
}

pub async fn crop(State(st): State<AppState>, _a: AdminUser, Path(face_id): Path<i64>) -> ApiResult<Response> {
    let path = engine_faces::crop_path(&st.engine.cache_dir, face_id);
    let bytes = tokio::fs::read(&path).await.map_err(|_| ApiError::NotFound)?;
    Ok((
        [
            (header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg")),
            (header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=2592000, immutable")),
        ],
        Body::from(bytes),
    )
        .into_response())
}

pub async fn media_faces(State(st): State<AppState>, _a: AdminUser, Path(media_id): Path<i64>) -> ApiResult<Json<Vec<MediaFace>>> {
    Ok(Json(faces::media_faces(&st.db, media_id).await?))
}
