use albumbox_core::Db;
use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::{config::Config, state::AppState};

async fn app_with_db() -> (Router, Db) {
    let data = tempfile::Builder::new().prefix("albumbox-test").tempdir().unwrap().keep();
    let db = Db::open(&data.join("test.db")).await.unwrap();
    let config = Config {
        lan_only: false,
        ui_dir: "does-not-exist".into(),
        data_dir: data,
        detect_interval_secs: 3600,
        ..Config::default()
    };
    let engine = albumbox_indexer::EngineHandle::start(db.clone(), config.engine());
    (super::router(AppState::new(db.clone(), config, engine)), db)
}

async fn app() -> Router {
    app_with_db().await.0
}

async fn call(app: &Router, method: Method, uri: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let req = match body {
        Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn setup_admin(app: &Router) -> String {
    let (s, body) = call(
        app,
        Method::POST,
        "/api/setup",
        None,
        Some(json!({ "username": "serhat", "display_name": "Serhat", "password": "gizli123" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{body}");
    body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn setup_runs_once() {
    let app = app().await;
    assert_eq!(call(&app, Method::GET, "/api/setup", None, None).await.1["needs_setup"], true);
    setup_admin(&app).await;
    assert_eq!(call(&app, Method::GET, "/api/setup", None, None).await.1["needs_setup"], false);
    let (s, body) = call(
        &app,
        Method::POST,
        "/api/setup",
        None,
        Some(json!({ "username": "baska", "display_name": "Başka", "password": "gizli123" })),
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
    assert_eq!(body["code"], "already_setup");
}

#[tokio::test]
async fn login_me_logout() {
    let app = app().await;
    setup_admin(&app).await;

    let (s, body) = call(&app, Method::POST, "/api/auth/login", None, Some(json!({ "username": "serhat", "password": "yanlis1" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "invalid_credentials");

    let (s, body) = call(&app, Method::POST, "/api/auth/login", None, Some(json!({ "username": "SERHAT", "password": "gizli123" }))).await;
    assert_eq!(s, StatusCode::OK);
    let token = body["token"].as_str().unwrap().to_string();

    let (s, me) = call(&app, Method::GET, "/api/auth/me", Some(&token), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(me["user"]["role"], "admin");
    assert_eq!(me["profile_ids"], json!([1]));

    assert_eq!(call(&app, Method::POST, "/api/auth/logout", Some(&token), None).await.0, StatusCode::OK);
    assert_eq!(call(&app, Method::GET, "/api/auth/me", Some(&token), None).await.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn member_cannot_use_admin_api() {
    let app = app().await;
    let admin = setup_admin(&app).await;

    let (s, _) = call(
        &app,
        Method::POST,
        "/api/admin/users",
        Some(&admin),
        Some(json!({ "username": "ahmet", "display_name": "Ahmet", "password": "123456", "role": "member" })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);

    let (_, body) = call(&app, Method::POST, "/api/auth/login", None, Some(json!({ "username": "ahmet", "password": "123456" }))).await;
    let member = body["token"].as_str().unwrap().to_string();

    assert_eq!(call(&app, Method::GET, "/api/admin/stats", Some(&member), None).await.0, StatusCode::FORBIDDEN);
    assert_eq!(call(&app, Method::GET, "/api/admin/stats", None, None).await.0, StatusCode::UNAUTHORIZED);
    let (s, home) = call(&app, Method::GET, "/api/home", Some(&member), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(home["visible_media"], 0);

    let (s, stats) = call(&app, Method::GET, "/api/admin/stats", Some(&admin), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(stats["users"], 2);
}

#[tokio::test]
async fn deactivated_user_is_signed_out_and_last_admin_is_protected() {
    let app = app().await;
    let admin = setup_admin(&app).await;
    let (_, created) = call(
        &app,
        Method::POST,
        "/api/admin/users",
        Some(&admin),
        Some(json!({ "username": "mehmet", "display_name": "Mehmet", "password": "123456", "role": "member" })),
    )
    .await;
    let id = created["id"].as_i64().unwrap();
    let (_, body) = call(&app, Method::POST, "/api/auth/login", None, Some(json!({ "username": "mehmet", "password": "123456" }))).await;
    let member = body["token"].as_str().unwrap().to_string();

    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/api/admin/users/{id}"),
        Some(&admin),
        Some(json!({ "display_name": "Mehmet", "role": "member", "is_active": false })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(call(&app, Method::GET, "/api/auth/me", Some(&member), None).await.0, StatusCode::UNAUTHORIZED);

    let (_, me) = call(&app, Method::GET, "/api/auth/me", Some(&admin), None).await;
    let admin_id = me["user"]["id"].as_i64().unwrap();
    let (s, body) = call(
        &app,
        Method::PUT,
        &format!("/api/admin/users/{admin_id}"),
        Some(&admin),
        Some(json!({ "display_name": "Serhat", "role": "member", "is_active": true })),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "cannot_demote_self");
}

#[tokio::test]
async fn unknown_api_route_is_json_404() {
    let app = app().await;
    let (s, body) = call(&app, Method::GET, "/api/yok", None, None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
}

async fn get_status(app: &Router, uri: &str, token: &str) -> StatusCode {
    call(app, Method::GET, uri, Some(token), None).await.0
}

#[tokio::test(flavor = "multi_thread")]
async fn library_scan_and_media_visibility() {
    let (app, db) = app_with_db().await;
    let admin = setup_admin(&app).await;
    call(
        &app,
        Method::POST,
        "/api/admin/users",
        Some(&admin),
        Some(json!({ "username": "ayse", "display_name": "Ayşe", "password": "123456", "role": "member" })),
    )
    .await;
    let (_, body) = call(&app, Method::POST, "/api/auth/login", None, Some(json!({ "username": "ayse", "password": "123456" }))).await;
    let member = body["token"].as_str().unwrap().to_string();

    // A temp folder plays the external disk.
    let disk = tempfile::tempdir().unwrap();
    let folder = disk.path().join("Ramazan_Tatili");
    std::fs::create_dir_all(&folder).unwrap();
    image::RgbImage::from_fn(400, 300, |x, _| image::Rgb([x as u8, 90, 160])).save(folder.join("IMG_20230421_101500.png")).unwrap();

    let path = disk.path().to_string_lossy().to_string();
    let (s, lib) = call(&app, Method::POST, "/api/admin/libraries", Some(&admin), Some(json!({ "path": path, "name": "Test Diski" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{lib}");
    let lib_id = lib["id"].as_str().unwrap().to_string();
    assert_eq!(get_status(&app, "/api/admin/libraries", &member).await, StatusCode::FORBIDDEN);

    // Wait until the scan job (including thumbnails) has finished.
    let mut finished = Value::Null;
    for _ in 0..300 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let (_, snap) = call(&app, Method::GET, "/api/admin/engine", Some(&admin), None).await;
        if snap["current"].is_null() && !snap["last_finished"].is_null() {
            finished = snap["last_finished"].clone();
            break;
        }
    }
    assert_eq!(finished["status"], "done", "{finished}");
    assert_eq!(finished["progress"]["thumbs_done"], 1);
    assert_eq!(get_status(&app, "/api/media/1/thumb/256", &admin).await, StatusCode::OK);

    let (_, folders) = call(&app, Method::GET, &format!("/api/admin/libraries/{lib_id}/folders"), Some(&admin), None).await;
    assert_eq!(folders[0]["suggestion"], "Ramazan Tatili");
    assert_eq!(folders[0]["sample_media_ids"], json!([1]));
    let (s, tagged) = call(
        &app,
        Method::POST,
        &format!("/api/admin/libraries/{lib_id}/folders/tag"),
        Some(&admin),
        Some(json!({ "folder_rel": "Ramazan_Tatili", "name": "Ramazan Tatili 2023" })),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(tagged["tagged"], 1);

    // New media is private: the member gets 404 for both thumbnail and original.
    assert_eq!(get_status(&app, "/api/media/1/thumb/256", &member).await, StatusCode::NOT_FOUND);
    assert_eq!(get_status(&app, "/api/media/1/original", &member).await, StatusCode::NOT_FOUND);
    assert_eq!(get_status(&app, "/api/media/1/original", &admin).await, StatusCode::OK);

    // Published to "Tüm Aile": now visible.
    sqlx::query("INSERT INTO media_access (profile_id, media_id) VALUES (1, 1)").execute(&db.pool).await.unwrap();
    assert_eq!(get_status(&app, "/api/media/1/thumb/1024", &member).await, StatusCode::OK);
    assert_eq!(get_status(&app, "/api/media/1/thumb/999", &member).await, StatusCode::NOT_FOUND);

    let (_, snap) = call(&app, Method::GET, "/api/admin/engine", Some(&admin), None).await;
    assert_eq!(snap["last_finished"]["status"], "done");
}

#[tokio::test(flavor = "multi_thread")]
async fn manage_categories_tags_profiles_and_bulk() {
    let (app, db) = app_with_db().await;
    let admin = setup_admin(&app).await;
    for (i, name) in ["a", "b", "c"].iter().enumerate() {
        sqlx::query("INSERT INTO media (id, hash, kind, taken_at) VALUES (?, ?, 'photo', ?)")
            .bind(i as i64 + 1)
            .bind(*name)
            .bind(format!("2022-0{}-01T10:00:00", i + 1))
            .execute(&db.pool)
            .await
            .unwrap();
    }
    let post = |uri: &'static str, body: Value| {
        let (app, admin) = (app.clone(), admin.clone());
        async move { call(&app, Method::POST, uri, Some(&admin), Some(body)).await }
    };

    let (s, cat) = post("/api/admin/categories", json!({ "name": "Köy" })).await;
    assert_eq!(s, StatusCode::CREATED);
    let cat_id = cat["id"].as_i64().unwrap();
    let (_, tag) = post("/api/admin/tags", json!({ "type_id": 3, "name": "Nişan 2022" })).await;
    let tag_id = tag["id"].as_i64().unwrap();
    let (_, again) = post("/api/admin/tags", json!({ "type_id": 3, "name": "NİŞAN 2022" })).await;
    assert_eq!(again["id"], tag_id, "existing tag returned");

    let (s, r) = post("/api/admin/media/bulk", json!({ "ids": [1, 2], "action": "add_tags", "tag_ids": [tag_id] })).await;
    assert_eq!((s, r["changed"].as_i64()), (StatusCode::OK, Some(2)));
    post("/api/admin/media/bulk", json!({ "ids": [1, 2, 3], "action": "add_to_category", "category_id": cat_id })).await;

    let (_, page) = call(&app, Method::GET, &format!("/api/admin/media?tags={tag_id}&limit=1"), Some(&admin), None).await;
    assert_eq!((page["total"].as_i64(), page["items"].as_array().unwrap().len()), (Some(2), 1));
    let (_, ids) = call(&app, Method::GET, "/api/admin/media/ids?q=nisan&sort=taken_asc", Some(&admin), None).await;
    assert_eq!(ids, json!([1, 2]));
    let (_, sugg) = call(&app, Method::GET, "/api/admin/tags?q=nis", Some(&admin), None).await;
    assert_eq!(sugg[0]["media_count"], 2);

    // Publish the category to a new profile with one member.
    let (_, prof) = post("/api/admin/profiles", json!({ "name": "Yakın Aile" })).await;
    let prof_id = prof["id"].as_i64().unwrap();
    let (_, user) = post(
        "/api/admin/users",
        json!({ "username": "veli", "display_name": "Veli", "password": "123456", "role": "member" }),
    )
    .await;
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/api/admin/profiles/{prof_id}/members"),
        Some(&admin),
        Some(json!({ "user_ids": [user["id"]] })),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(
        &app,
        Method::PUT,
        &format!("/api/admin/categories/{cat_id}/shares"),
        Some(&admin),
        Some(json!({ "profile_ids": [prof_id] })),
    )
    .await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    post("/api/admin/media/bulk", json!({ "ids": [3], "action": "set_visibility", "mode": "private" })).await;

    let (_, body) = call(&app, Method::POST, "/api/auth/login", None, Some(json!({ "username": "veli", "password": "123456" }))).await;
    let veli = body["token"].as_str().unwrap().to_string();
    let (_, home) = call(&app, Method::GET, "/api/home", Some(&veli), None).await;
    assert_eq!((home["visible_media"].as_i64(), home["album_count"].as_i64()), (Some(2), Some(1)));
    let (_, found) = call(&app, Method::GET, "/api/search?q=nisan", Some(&veli), None).await;
    assert_eq!((found["total"].as_i64(), found["groups"][0]["name"].as_str()), (Some(2), Some("Nişan 2022")));
    let key = found["groups"][0]["key"].as_str().unwrap().to_string();
    let (_, page) = call(&app, Method::GET, &format!("/api/search/media?q=nisan&key={key}"), Some(&veli), None).await;
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    let (_, albums) = call(&app, Method::GET, "/api/albums", Some(&veli), None).await;
    assert_eq!(albums[0]["media_count"], 2);
    assert_eq!(call(&app, Method::GET, "/api/media/3", Some(&veli), None).await.0, StatusCode::NOT_FOUND);

    let (_, detail) = call(&app, Method::GET, "/api/admin/media/1", Some(&admin), None).await;
    assert_eq!(detail["access_profile_ids"], json!([prof_id]));
    assert_eq!(detail["tags"][0]["type_code"], "event");
    let (_, profiles) = call(&app, Method::GET, "/api/admin/profiles", Some(&admin), None).await;
    assert_eq!(profiles[1]["visible_media"], 2);
    assert_eq!(call(&app, Method::GET, "/api/admin/categories", Some(&veli), None).await.0, StatusCode::FORBIDDEN);
}
