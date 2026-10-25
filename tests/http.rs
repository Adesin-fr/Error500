mod common;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use common::TestDb;
use error500::http::{router, AppState};
use error500::projects::{self, Project};
use error500::storage::BlobStore;
use error500::users;
use flate2::write::GzEncoder;
use flate2::Compression;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::io::Write;
use tower::ServiceExt;

const EVENT_ID: &str = "9ec79c33ec9942ab8353589fcb2e04dc";

struct App {
    db: TestDb,
    store: BlobStore,
    project: Project,
    router: Router,
}

async fn app_with_limit(max_body: usize) -> App {
    let db = TestDb::new().await;
    let u = users::create_user(&db.pool, "a@example.com", "A", "correct horse").await.unwrap();
    let project = projects::create_project(&db.pool, u.id, "App").await.unwrap();
    let store = BlobStore::memory();
    let router = router(AppState { pool: db.pool.clone(), store: store.clone(), max_body });
    App { db, store, project, router }
}

async fn app() -> App {
    app_with_limit(1024 * 1024).await
}

fn event_json() -> Value {
    json!({"event_id": EVENT_ID, "level": "error", "timestamp": 1700000000,
           "exception": {"values": [{"type": "Error", "value": "boom"}]}})
}

fn envelope_body() -> String {
    let ev = event_json().to_string();
    format!(
        "{{\"event_id\":\"{EVENT_ID}\"}}\n{{\"type\":\"event\",\"length\":{}}}\n{ev}\n",
        ev.len()
    )
}

async fn send(router: &Router, req: Request<Body>) -> (StatusCode, axum::http::HeaderMap, Value) {
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, headers, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn post(uri: &str, auth_key: Option<&str>, body: impl Into<Body>) -> Request<Body> {
    let mut b = Request::builder().method(Method::POST).uri(uri);
    if let Some(k) = auth_key {
        b = b.header("X-Sentry-Auth", format!("Sentry sentry_version=7, sentry_key={k}"));
    }
    b.body(body.into()).unwrap()
}

async fn event_count(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM events").fetch_one(&db.pool).await.unwrap()
}

#[tokio::test]
async fn envelope_with_auth_header_is_accepted_and_stored() {
    let a = app().await;
    let uri = format!("/api/{}/envelope/", a.project.id);
    let (status, _, body) = send(&a.router, post(&uri, Some(&a.project.public_key), envelope_body())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], EVENT_ID);
    assert_eq!(event_count(&a.db).await, 1);
    a.db.cleanup().await;
}

#[tokio::test]
async fn path_without_trailing_slash_also_works() {
    let a = app().await;
    let uri = format!("/api/{}/envelope", a.project.id);
    let (status, ..) = send(&a.router, post(&uri, Some(&a.project.public_key), envelope_body())).await;
    assert_eq!(status, StatusCode::OK);
    a.db.cleanup().await;
}

#[tokio::test]
async fn key_can_come_from_query_string() {
    let a = app().await;
    let uri = format!("/api/{}/envelope/?sentry_version=7&sentry_key={}", a.project.id, a.project.public_key);
    let (status, ..) = send(&a.router, post(&uri, None, envelope_body())).await;
    assert_eq!(status, StatusCode::OK);
    a.db.cleanup().await;
}

#[tokio::test]
async fn key_can_come_from_envelope_header_dsn() {
    let a = app().await;
    let ev = event_json().to_string();
    let body = format!(
        "{{\"event_id\":\"{EVENT_ID}\",\"dsn\":\"http://{}@localhost/{}\"}}\n{{\"type\":\"event\",\"length\":{}}}\n{ev}\n",
        a.project.public_key, a.project.id, ev.len()
    );
    let uri = format!("/api/{}/envelope/", a.project.id);
    let (status, ..) = send(&a.router, post(&uri, None, body)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(event_count(&a.db).await, 1);
    a.db.cleanup().await;
}

#[tokio::test]
async fn missing_key_is_401_and_nothing_is_stored() {
    let a = app().await;
    let uri = format!("/api/{}/envelope/", a.project.id);
    let (status, ..) = send(&a.router, post(&uri, None, envelope_body())).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(event_count(&a.db).await, 0);
    a.db.cleanup().await;
}

#[tokio::test]
async fn wrong_key_or_wrong_project_is_403() {
    let a = app().await;
    let uri = format!("/api/{}/envelope/", a.project.id);
    let (status, ..) = send(&a.router, post(&uri, Some("deadbeef"), envelope_body())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, ..) = send(&a.router, post("/api/9999/envelope/", Some(&a.project.public_key), envelope_body())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(event_count(&a.db).await, 0);
    a.db.cleanup().await;
}

#[tokio::test]
async fn gzip_encoded_body_is_accepted() {
    let a = app().await;
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(envelope_body().as_bytes()).unwrap();
    let gz = enc.finish().unwrap();
    let uri = format!("/api/{}/envelope/", a.project.id);
    let mut req = post(&uri, Some(&a.project.public_key), gz);
    req.headers_mut().insert(header::CONTENT_ENCODING, "gzip".parse().unwrap());
    let (status, ..) = send(&a.router, req).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(event_count(&a.db).await, 1);
    a.db.cleanup().await;
}

#[tokio::test]
async fn malformed_envelope_is_400() {
    let a = app().await;
    let uri = format!("/api/{}/envelope/", a.project.id);
    let (status, _, body) = send(&a.router, post(&uri, Some(&a.project.public_key), "not json\n")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["detail"].is_string());
    a.db.cleanup().await;
}

#[tokio::test]
async fn oversized_body_is_413() {
    let a = app_with_limit(64).await;
    let uri = format!("/api/{}/envelope/", a.project.id);
    let (status, ..) = send(&a.router, post(&uri, Some(&a.project.public_key), envelope_body())).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    a.db.cleanup().await;
}

#[tokio::test]
async fn gzip_bomb_is_413() {
    let a = app_with_limit(4096).await;
    let mut enc = GzEncoder::new(Vec::new(), Compression::best());
    enc.write_all(&vec![b'a'; 2 * 1024 * 1024]).unwrap();
    let gz = enc.finish().unwrap();
    assert!(gz.len() < 4096, "compressed {}", gz.len());
    let uri = format!("/api/{}/envelope/", a.project.id);
    let mut req = post(&uri, Some(&a.project.public_key), gz);
    req.headers_mut().insert(header::CONTENT_ENCODING, "gzip".parse().unwrap());
    let (status, ..) = send(&a.router, req).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    a.db.cleanup().await;
}

#[tokio::test]
async fn legacy_store_endpoint_accepts_event_json() {
    let a = app().await;
    let uri = format!("/api/{}/store/", a.project.id);
    let (status, _, body) = send(&a.router, post(&uri, Some(&a.project.public_key), event_json().to_string())).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["id"], EVENT_ID);
    assert_eq!(event_count(&a.db).await, 1);
    a.db.cleanup().await;
}

#[tokio::test]
async fn cors_preflight_allows_sentry_headers_from_any_origin() {
    let a = app().await;
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri(format!("/api/{}/envelope/", a.project.id))
        .header(header::ORIGIN, "https://app.example.com")
        .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
        .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "x-sentry-auth,content-type")
        .body(Body::empty())
        .unwrap();
    let (status, headers, _) = send(&a.router, req).await;
    assert!(status.is_success());
    assert_eq!(headers[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    let allowed = headers[header::ACCESS_CONTROL_ALLOW_HEADERS].to_str().unwrap().to_lowercase();
    assert!(allowed.contains("x-sentry-auth"), "got {allowed}");
    a.db.cleanup().await;
}

#[tokio::test]
async fn replay_envelope_over_http_stores_the_segment_blob() {
    let a = app().await;
    let rid = "22222222222222222222222222222222";
    let re = json!({"type": "replay_event", "replay_id": rid, "segment_id": 0, "timestamp": 1700000000.0}).to_string();
    let rec = "{\"segment_id\":0}\n[{\"type\":2}]";
    let body = format!(
        "{{\"event_id\":\"{rid}\"}}\n{{\"type\":\"replay_event\",\"length\":{}}}\n{re}\n{{\"type\":\"replay_recording\",\"length\":{}}}\n{rec}\n",
        re.len(), rec.len()
    );
    let uri = format!("/api/{}/envelope/", a.project.id);
    let (status, ..) = send(&a.router, post(&uri, Some(&a.project.public_key), body)).await;
    assert_eq!(status, StatusCode::OK);
    let blob = a.store.get(&format!("replays/{}/{rid}/0", a.project.id)).await.unwrap();
    assert_eq!(blob.unwrap().as_ref(), b"[{\"type\":2}]");
    a.db.cleanup().await;
}

#[tokio::test]
async fn health_endpoint_is_ok() {
    let a = app().await;
    let resp = a.router.clone().oneshot(Request::get("/health").body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    a.db.cleanup().await;
}

// --- Upload de source maps ---

const MAP: &str = r#"{"version":3,"sources":["src/app.ts"],"names":[],"mappings":"AAAA"}"#;

fn upload(project_id: u64, query: &str, token: Option<&str>, body: &str) -> Request<Body> {
    let mut b = Request::builder().method(Method::POST).uri(format!("/api/{project_id}/sourcemaps/{query}"));
    if let Some(t) = token {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::from(body.to_string())).unwrap()
}

async fn map_count(db: &TestDb) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sourcemaps").fetch_one(&db.pool).await.unwrap()
}

#[tokio::test]
async fn sourcemap_upload_with_project_token_is_stored() {
    let a = app().await;
    let token = projects::upload_token(&a.db.pool, a.project.id).await.unwrap();
    let q = "?release=1.0&name=~%2Fstatic%2Fapp.min.js.map";
    let (status, ..) = send(&a.router, upload(a.project.id, q, Some(&token), MAP)).await;
    assert_eq!(status, StatusCode::OK);
    let (name, release): (String, String) = sqlx::query_as("SELECT name, release_key FROM sourcemaps").fetch_one(&a.db.pool).await.unwrap();
    assert_eq!((name.as_str(), release.as_str()), ("~/static/app.min.js.map", "1.0"));
    a.db.cleanup().await;
}

#[tokio::test]
async fn sourcemap_upload_requires_a_valid_token() {
    let a = app().await;
    let q = "?name=~%2Fa.js.map";
    let (status, ..) = send(&a.router, upload(a.project.id, q, None, MAP)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let (status, ..) = send(&a.router, upload(a.project.id, q, Some("wrong"), MAP)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // la clé publique d'ingestion ne donne pas le droit d'uploader
    let (status, ..) = send(&a.router, upload(a.project.id, q, Some(&a.project.public_key), MAP)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(map_count(&a.db).await, 0);
    a.db.cleanup().await;
}

#[tokio::test]
async fn sourcemap_upload_rejects_invalid_map_and_missing_name() {
    let a = app().await;
    let token = projects::upload_token(&a.db.pool, a.project.id).await.unwrap();
    let (status, ..) = send(&a.router, upload(a.project.id, "?name=~%2Fa.js.map", Some(&token), "nope")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, ..) = send(&a.router, upload(a.project.id, "", Some(&token), MAP)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(map_count(&a.db).await, 0);
    a.db.cleanup().await;
}
