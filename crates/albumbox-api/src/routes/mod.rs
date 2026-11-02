mod admin;
mod client;
mod engine;
mod faces;
mod libraries;
mod manage;
mod media;
mod session;
mod setup;

use std::path::Path;

use axum::{
    http::{header, HeaderValue},
    middleware,
    response::{Html, IntoResponse},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;
use tower_http::{
    compression::CompressionLayer,
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

use crate::{error::ApiError, lan, state::AppState};

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health", get(health))
        .route("/setup", get(setup::status).post(setup::create_admin))
        .route("/auth/login", post(session::login))
        .route("/auth/logout", post(session::logout))
        .route("/auth/me", get(session::me))
        .route("/home", get(client::home))
        .route("/search", get(client::search))
        .route("/search/media", get(client::search_media))
        .route("/albums", get(client::albums))
        .route("/albums/{id}/media", get(client::album_media))
        .route("/media/{id}", get(client::media_info))
        .route("/media/{id}/thumb/{size}", get(media::thumb))
        .route("/media/{id}/original", get(media::original))
        .route("/admin/stats", get(admin::stats))
        .route("/admin/users", get(admin::list_users).post(admin::create_user))
        .route("/admin/users/{id}", put(admin::update_user))
        .route("/admin/users/{id}/password", put(admin::reset_password))
        .route("/admin/libraries", get(libraries::list).post(libraries::add))
        .route("/admin/libraries/refresh", post(libraries::refresh))
        .route("/admin/libraries/{id}", put(libraries::rename))
        .route("/admin/libraries/{id}/scan", post(libraries::scan))
        .route("/admin/libraries/{id}/folders", get(libraries::folders))
        .route("/admin/libraries/{id}/folders/tag", post(libraries::tag_folder))
        .route("/admin/fs", get(libraries::browse))
        .route("/admin/media", get(manage::media_list))
        .route("/admin/media/ids", get(manage::media_ids))
        .route("/admin/media/bulk", post(manage::media_bulk))
        .route("/admin/media/{id}", get(manage::media_detail))
        .route("/admin/categories", get(manage::category_list).post(manage::category_create))
        .route("/admin/categories/{id}", put(manage::category_update).delete(manage::category_delete))
        .route("/admin/categories/{id}/move", put(manage::category_move))
        .route("/admin/categories/{id}/shares", put(manage::category_shares))
        .route("/admin/categories/{id}/cover", put(manage::category_cover))
        .route("/admin/tag-types", get(manage::type_list).post(manage::type_create))
        .route("/admin/tag-types/{id}", put(manage::type_update).delete(manage::type_delete))
        .route("/admin/tags", get(manage::tag_list).post(manage::tag_create))
        .route("/admin/tags/{id}", put(manage::tag_update).delete(manage::tag_delete))
        .route("/admin/tags/{id}/merge", post(manage::tag_merge))
        .route("/admin/profiles", get(manage::profile_list).post(manage::profile_create))
        .route("/admin/profiles/{id}", put(manage::profile_update).delete(manage::profile_delete))
        .route("/admin/profiles/{id}/members", put(manage::profile_members))
        .route("/admin/faces/status", get(faces::status))
        .route("/admin/faces/scan", post(faces::scan))
        .route("/admin/faces/clusters", get(faces::clusters))
        .route("/admin/faces/clusters/{id}/name", post(faces::name_cluster))
        .route("/admin/faces/clusters/{id}/ignore", post(faces::ignore_cluster))
        .route("/admin/faces/persons", get(faces::persons))
        .route("/admin/faces/persons/{id}", get(faces::person_faces))
        .route("/admin/faces/confirm", post(faces::confirm))
        .route("/admin/faces/reject", post(faces::reject))
        .route("/admin/faces/{id}/crop", get(faces::crop))
        .route("/admin/media/{id}/faces", get(faces::media_faces))
        .route("/admin/engine", get(engine::snapshot))
        .route("/admin/events", get(engine::events))
        .route("/admin/jobs", get(engine::recent))
        .route("/admin/jobs/{id}/cancel", post(engine::cancel))
        .fallback(|| async { ApiError::NotFound })
        // Per-user data: not cacheable unless a handler says otherwise (thumbnails).
        .layer(SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, HeaderValue::from_static("no-store")))
        .with_state(state.clone());

    let mut app = Router::new().nest("/api", api);
    app = with_ui(app, &state.config.ui_dir);

    let app = app.layer(CompressionLayer::new()).layer(TraceLayer::new_for_http());
    if state.config.lan_only { app.layer(middleware::from_fn(lan::lan_only)) } else { app }
}

/// Serves the built Svelte app; unknown paths fall back to index.html for client-side routing.
fn with_ui(app: Router, ui_dir: &Path) -> Router {
    let index = ui_dir.join("index.html");
    if !index.exists() {
        tracing::warn!(dir = %ui_dir.display(), "UI build not found; serving API only");
        return app.fallback(|| async {
            Html("<h3>AlbumBox API çalışıyor.</h3><p>Arayüz derlemesi bulunamadı (ui/dist).</p>")
        });
    }
    tracing::info!(dir = %ui_dir.display(), "serving UI");
    app.fallback_service(ServeDir::new(ui_dir).fallback(ServeFile::new(index)))
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

#[cfg(test)]
mod tests;
