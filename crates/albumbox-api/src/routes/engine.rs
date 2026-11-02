//! Admin: background job status. `/admin/events` streams engine snapshots (SSE).

use std::{convert::Infallible, time::Duration};

use albumbox_core::jobs::{self, Job};
use albumbox_indexer::EngineSnapshot;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures::Stream;

use crate::{
    auth::AdminUser,
    error::{ApiError, ApiResult},
    state::AppState,
};

pub async fn snapshot(State(st): State<AppState>, _a: AdminUser) -> Json<EngineSnapshot> {
    Json(st.engine.snapshot())
}

/// Sends the current snapshot, then a new one whenever it changes, at most ~4/s.
pub async fn events(State(st): State<AppState>, _a: AdminUser) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = st.engine.subscribe();
    let stream = futures::stream::unfold((rx, st.engine.clone(), true), |(mut rx, engine, first)| async move {
        if !first {
            rx.changed().await.ok()?;
            // Coalesce bursts of progress updates.
            tokio::time::sleep(Duration::from_millis(250)).await;
            rx.borrow_and_update();
        }
        let event = Event::default().event("engine").json_data(engine.snapshot()).ok()?;
        Some((Ok(event), (rx, engine, false)))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

pub async fn recent(State(st): State<AppState>, _a: AdminUser) -> ApiResult<Json<Vec<Job>>> {
    Ok(Json(jobs::recent(&st.db, 20).await?))
}

pub async fn cancel(State(st): State<AppState>, _a: AdminUser, Path(id): Path<i64>) -> ApiResult<StatusCode> {
    if st.engine.cancel_running(id) || jobs::cancel_queued(&st.db, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}
