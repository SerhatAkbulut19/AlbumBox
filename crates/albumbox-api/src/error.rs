use albumbox_core::CoreError;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

/// API error. The UI maps `code` to a Turkish message.
#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Forbidden,
    NotFound,
    BadRequest(&'static str),
    Conflict(&'static str),
    TooManyRequests,
    Internal(anyhow::Error),
}

pub type ApiResult<T> = Result<T, ApiError>;

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            ApiError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::BadRequest(code) => (StatusCode::BAD_REQUEST, *code),
            ApiError::Conflict(code) => (StatusCode::CONFLICT, *code),
            ApiError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "too_many_attempts"),
            ApiError::Internal(err) => {
                tracing::error!(error = ?err, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        (status, Json(json!({ "code": code }))).into_response()
    }
}

impl From<CoreError> for ApiError {
    fn from(e: CoreError) -> Self {
        match e {
            CoreError::NotFound => ApiError::NotFound,
            CoreError::Conflict(code) => ApiError::Conflict(code),
            CoreError::Invalid(code) => ApiError::BadRequest(code),
            other => ApiError::Internal(other.into()),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::Internal(e)
    }
}

impl From<tokio::task::JoinError> for ApiError {
    fn from(e: tokio::task::JoinError) -> Self {
        ApiError::Internal(e.into())
    }
}
