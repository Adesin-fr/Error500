//! Protocole d'upload de `sentry-cli sourcemaps upload` : chunk-upload + artifactbundle/assemble.

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
use sha1::{Digest, Sha1};
use std::io::Write;
use tower::ServiceExt;

const MAP: &str = r#"{"version":3,"sources":["src/app.ts"],"names":[],"mappings":"AAAA"}"#;
const DEBUG_ID: &str = "5a3c8a2e-1f7d-4b9e-9d3a-0c1b2d3e4f50";

struct App {
    db: TestDb,
    project: Project,
    token: String,
    router: Router,
}

async fn app() -> App {
    let db = TestDb::new().await;
    let u = users::create_user(&db.pool, "a@example.com", "A", "correct horse").await.unwrap();
    let project = projects::create_project(&db.pool, u.id, "App").await.unwrap();
    let token = projects::upload_token(&db.pool, project.id).await.unwrap();
    let router = router(AppState { pool: db.pool.clone(), store: BlobStore::memory(), max_body: 1024 * 1024, mailer: error500::alerts::Mailer::disabled(), public_url: "http://localhost".into() });
    App { db, project, token, router }
}

fn sha1_hex(data: &[u8]) -> String {
    hex::encode(Sha1::digest(data))
}

/// Archive « artifact bundle » comme celle de sentry-cli : `manifest.json` + fichiers.
fn bundle(map_url: &str, map_body: &str, header_debug_id: Option<&str>) -> Vec<u8> {
    let mut headers = json!({});
    if let Some(id) = header_debug_id {
        headers["debug-id"] = json!(id);
    }
    let manifest = json!({
        "files": {
            "files/_/_/app.min.js.map": {"type": "source_map", "url": map_url, "headers": headers},
            "files/_/_/app.min.js": {"type": "minified_source", "url": "~/static/app.min.js", "headers": {}},
        }
    });
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default();
    for (name, body) in [
        ("manifest.json", manifest.to_string()),
        ("files/_/_/app.min.js.map", map_body.to_string()),
        ("files/_/_/app.min.js", "console.log(1)".to_string()),
    ] {
        zip.start_file(name, opts).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn authed(method: Method, uri: &str, token: Option<&str>, content_type: &str, body: Vec<u8>) -> Request<Body> {
    let mut b = Request::builder().method(method).uri(uri).header(header::CONTENT_TYPE, content_type);
    if let Some(t) = token {
        b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    b.body(Body::from(body)).unwrap()
}

async fn send(router: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = router.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn options(a: &App) -> Value {
    let req = Request::get("/api/0/organizations/acme/chunk-upload/")
        .header(header::AUTHORIZATION, format!("Bearer {}", a.token))
        .body(Body::empty())
        .unwrap();
    send(&a.router, req).await.1
}

/// Découpe l'archive comme le fait le client : `(sha1 de l'archive, [(sha1, octets)])`.
fn chunked(zip: &[u8], chunk_size: usize) -> (String, Vec<(String, Vec<u8>)>) {
    let chunks = zip.chunks(chunk_size).map(|c| (sha1_hex(c), c.to_vec())).collect();
    (sha1_hex(zip), chunks)
}

fn multipart(parts: &[(&str, &str, Vec<u8>)]) -> (String, Vec<u8>) {
    let boundary = "----e500boundary";
    let mut body = Vec::new();
    for (field, filename, data) in parts {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{field}\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
        body.extend_from_slice(data);
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (format!("multipart/form-data; boundary={boundary}"), body)
}

fn gzip(data: &[u8]) -> Vec<u8> {
    let mut e = GzEncoder::new(Vec::new(), Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

async fn upload_chunks(a: &App, chunks: &[(String, Vec<u8>)], gzipped: bool) -> StatusCode {
    let parts: Vec<(&str, &str, Vec<u8>)> = chunks
        .iter()
        .map(|(sum, data)| if gzipped { ("file_gzip", sum.as_str(), gzip(data)) } else { ("file", sum.as_str(), data.clone()) })
        .collect();
    let (ct, body) = multipart(&parts);
    send(&a.router, authed(Method::POST, "/api/0/organizations/acme/chunk-upload/", Some(&a.token), &ct, body)).await.0
}

async fn assemble(a: &App, checksum: &str, chunks: &[(String, Vec<u8>)], version: Option<&str>) -> (StatusCode, Value) {
    let ids: Vec<&str> = chunks.iter().map(|(s, _)| s.as_str()).collect();
    let mut body = json!({"checksum": checksum, "chunks": ids, "projects": ["app"]});
    if let Some(v) = version {
        body["version"] = json!(v);
    }
    send(
        &a.router,
        authed(Method::POST, "/api/0/organizations/acme/artifactbundle/assemble/", Some(&a.token), "application/json", body.to_string().into_bytes()),
    )
    .await
}

async fn maps(a: &App) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as("SELECT name, release_key, debug_id FROM sourcemaps ORDER BY name").fetch_all(&a.db.pool).await.unwrap()
}

#[tokio::test]
async fn chunk_upload_options_announce_artifact_bundles_with_sha1() {
    let a = app().await;
    let o = options(&a).await;
    assert_eq!(o["hashAlgorithm"], "sha1");
    assert!(o["accept"].as_array().unwrap().iter().any(|v| v == "artifact_bundles"));
    assert!(o["compression"].as_array().unwrap().iter().any(|v| v == "gzip"));
    assert!(o["chunkSize"].as_u64().unwrap() > 0);
    assert!(o["chunksPerRequest"].as_u64().unwrap() > 0);
    assert!(o["url"].as_str().unwrap().ends_with("/api/0/organizations/acme/chunk-upload/"));
    a.db.cleanup().await;
}

#[tokio::test]
async fn full_sentry_cli_flow_stores_the_source_map() {
    let a = app().await;
    let (checksum, chunks) = chunked(&bundle("~/static/app.min.js.map", MAP, None), 200);
    assert!(chunks.len() > 1, "le test doit exercer plusieurs chunks");

    let (status, r) = assemble(&a, &checksum, &chunks, Some("1.0")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(r["state"], "not_found");
    assert_eq!(r["missingChunks"].as_array().unwrap().len(), chunks.len());

    assert_eq!(upload_chunks(&a, &chunks, true).await, StatusCode::OK);

    let (status, r) = assemble(&a, &checksum, &chunks, Some("1.0")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(r["state"], "ok", "{r}");
    assert_eq!(maps(&a).await, vec![("~/static/app.min.js.map".to_string(), "1.0".to_string(), None)]);

    // ré-assemblage idempotent
    let (_, r) = assemble(&a, &checksum, &chunks, Some("1.0")).await;
    assert_eq!(r["state"], "ok");
    assert_eq!(maps(&a).await.len(), 1);
    a.db.cleanup().await;
}

#[tokio::test]
async fn only_missing_chunks_are_requested_and_uncompressed_chunks_work() {
    let a = app().await;
    let (checksum, chunks) = chunked(&bundle("~/static/app.min.js.map", MAP, None), 200);
    assert_eq!(upload_chunks(&a, &chunks[..1], false).await, StatusCode::OK);
    let (_, r) = assemble(&a, &checksum, &chunks, None).await;
    assert_eq!(r["state"], "not_found");
    let missing: Vec<&str> = r["missingChunks"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert_eq!(missing, chunks[1..].iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>());
    a.db.cleanup().await;
}

#[tokio::test]
async fn debug_id_comes_from_the_manifest_headers_when_the_map_has_none() {
    let a = app().await;
    let (checksum, chunks) = chunked(&bundle("~/static/app.min.js.map", MAP, Some(DEBUG_ID)), 4096);
    upload_chunks(&a, &chunks, true).await;
    let (_, r) = assemble(&a, &checksum, &chunks, Some("2.0")).await;
    assert_eq!(r["state"], "ok", "{r}");
    assert_eq!(maps(&a).await[0].2.as_deref(), Some(DEBUG_ID));
    a.db.cleanup().await;
}

#[tokio::test]
async fn checksum_mismatch_is_an_error_and_stores_nothing() {
    let a = app().await;
    let (_, chunks) = chunked(&bundle("~/static/app.min.js.map", MAP, None), 4096);
    upload_chunks(&a, &chunks, true).await;
    let (status, r) = assemble(&a, &sha1_hex(b"autre"), &chunks, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(r["state"], "error");
    assert!(maps(&a).await.is_empty());
    a.db.cleanup().await;
}

#[tokio::test]
async fn invalid_archive_is_an_error() {
    let a = app().await;
    let junk = b"ceci n'est pas un zip".to_vec();
    let (checksum, chunks) = chunked(&junk, 4096);
    upload_chunks(&a, &chunks, false).await;
    let (_, r) = assemble(&a, &checksum, &chunks, None).await;
    assert_eq!(r["state"], "error");
    a.db.cleanup().await;
}

#[tokio::test]
async fn a_chunk_whose_content_does_not_match_its_name_is_rejected() {
    let a = app().await;
    let (ct, body) = multipart(&[("file", &sha1_hex(b"attendu"), b"recu".to_vec())]);
    let (status, _) = send(&a.router, authed(Method::POST, "/api/0/organizations/acme/chunk-upload/", Some(&a.token), &ct, body)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    a.db.cleanup().await;
}

#[tokio::test]
async fn chunks_are_scoped_to_the_project() {
    let a = app().await;
    let (checksum, chunks) = chunked(&bundle("~/static/app.min.js.map", MAP, None), 4096);
    upload_chunks(&a, &chunks, true).await;
    // un second projet ne voit pas les chunks du premier
    let u = users::find_by_email(&a.db.pool, "a@example.com").await.unwrap().unwrap();
    let other = projects::create_project(&a.db.pool, u.id, "Autre").await.unwrap();
    let other_token = projects::upload_token(&a.db.pool, other.id).await.unwrap();
    let ids: Vec<&str> = chunks.iter().map(|(s, _)| s.as_str()).collect();
    let body = json!({"checksum": checksum, "chunks": ids}).to_string().into_bytes();
    let (_, r) = send(
        &a.router,
        authed(Method::POST, "/api/0/organizations/acme/artifactbundle/assemble/", Some(&other_token), "application/json", body),
    )
    .await;
    assert_eq!(r["state"], "not_found");
    a.db.cleanup().await;
}

#[tokio::test]
async fn endpoints_require_a_valid_upload_token() {
    let a = app().await;
    let get = |token: Option<&str>| {
        let mut b = Request::get("/api/0/organizations/acme/chunk-upload/");
        if let Some(t) = token {
            b = b.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        b.body(Body::empty()).unwrap()
    };
    assert_eq!(send(&a.router, get(None)).await.0, StatusCode::UNAUTHORIZED);
    assert_eq!(send(&a.router, get(Some("faux"))).await.0, StatusCode::FORBIDDEN);
    // la clé publique du DSN n'ouvre pas ces routes
    assert_eq!(send(&a.router, get(Some(&a.project.public_key))).await.0, StatusCode::FORBIDDEN);
    let body = json!({"checksum": sha1_hex(b"x"), "chunks": []}).to_string().into_bytes();
    let (status, _) = send(
        &a.router,
        authed(Method::POST, "/api/0/organizations/acme/artifactbundle/assemble/", None, "application/json", body),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    a.db.cleanup().await;
}

#[tokio::test]
async fn release_creation_and_finalization_are_accepted() {
    let a = app().await;
    let body = json!({"version": "1.0", "projects": ["app"]}).to_string().into_bytes();
    let (status, r) = send(&a.router, authed(Method::POST, "/api/0/organizations/acme/releases/", Some(&a.token), "application/json", body)).await;
    assert!(status == StatusCode::CREATED || status == StatusCode::OK, "{status}");
    assert_eq!(r["version"], "1.0");
    let body = json!({"dateReleased": "2026-01-01T00:00:00Z"}).to_string().into_bytes();
    let (status, _) = send(&a.router, authed(Method::PUT, "/api/0/organizations/acme/releases/1.0/", Some(&a.token), "application/json", body)).await;
    assert_eq!(status, StatusCode::OK);
    a.db.cleanup().await;
}
