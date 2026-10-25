mod common;

use bytes::Bytes;
use common::TestDb;
use error500::projects::{self, Project};
use error500::sourcemaps::{self, SourceMapError};
use error500::storage::BlobStore;
use error500::users;
use serde_json::json;

const MAP: &str = r#"{"version":3,"sources":["src/app.ts"],"names":["handleClick"],
 "sourcesContent":["line1\nline2\nfunction handleClick() {\n  throw 1\n}\nline6"],"mappings":"SAEIA"}"#;

fn map_with_debug_id(id: &str) -> String {
    let mut v: serde_json::Value = serde_json::from_str(MAP).unwrap();
    v["debugId"] = json!(id);
    v.to_string()
}

async fn setup() -> (TestDb, BlobStore, Project) {
    let db = TestDb::new().await;
    let u = users::create_user(&db.pool, "a@example.com", "A", "correct horse").await.unwrap();
    let p = projects::create_project(&db.pool, u.id, "App").await.unwrap();
    (db, BlobStore::memory(), p)
}

fn event(release: Option<&str>, abs_path: &str) -> serde_json::Value {
    let mut ev = json!({"exception": {"values": [{"type": "Error", "value": "x",
        "stacktrace": {"frames": [{"abs_path": abs_path, "lineno": 1, "colno": 10}]}}]}});
    if let Some(r) = release {
        ev["release"] = json!(r);
    }
    ev
}

const URL: &str = "https://app.example.com/static/app.min.js";

#[tokio::test]
async fn map_is_found_by_release_and_artifact_name() {
    let (db, store, p) = setup().await;
    sourcemaps::store_sourcemap(&db.pool, &store, p.id, Some("1.0"), "~/static/app.min.js.map", Bytes::from(MAP)).await.unwrap();
    let set = sourcemaps::load_for_event(&db.pool, &store, p.id, &event(Some("1.0"), URL)).await.unwrap();
    assert!(set.get(URL).is_some());
    db.cleanup().await;
}

#[tokio::test]
async fn map_without_release_matches_any_release() {
    let (db, store, p) = setup().await;
    sourcemaps::store_sourcemap(&db.pool, &store, p.id, None, "~/static/app.min.js.map", Bytes::from(MAP)).await.unwrap();
    let set = sourcemaps::load_for_event(&db.pool, &store, p.id, &event(Some("9.9"), URL)).await.unwrap();
    assert!(set.get(URL).is_some());
    db.cleanup().await;
}

#[tokio::test]
async fn map_of_another_release_does_not_match() {
    let (db, store, p) = setup().await;
    sourcemaps::store_sourcemap(&db.pool, &store, p.id, Some("1.0"), "~/static/app.min.js.map", Bytes::from(MAP)).await.unwrap();
    let set = sourcemaps::load_for_event(&db.pool, &store, p.id, &event(Some("2.0"), URL)).await.unwrap();
    assert!(set.get(URL).is_none());
    db.cleanup().await;
}

#[tokio::test]
async fn map_is_found_by_debug_id_whatever_its_name() {
    let (db, store, p) = setup().await;
    let id = "5a3c8a2e-1f7d-4b9e-9d3a-0c1b2d3e4f50";
    sourcemaps::store_sourcemap(&db.pool, &store, p.id, None, "~/whatever/other.map", Bytes::from(map_with_debug_id(id))).await.unwrap();
    let mut ev = event(None, URL);
    ev["debug_meta"] = json!({"images": [{"type": "sourcemap", "code_file": URL, "debug_id": id}]});
    let set = sourcemaps::load_for_event(&db.pool, &store, p.id, &ev).await.unwrap();
    assert!(set.get(URL).is_some());
    db.cleanup().await;
}

#[tokio::test]
async fn maps_are_isolated_between_projects() {
    let (db, store, p) = setup().await;
    let other = projects::create_project(&db.pool, 1, "Other").await.unwrap();
    sourcemaps::store_sourcemap(&db.pool, &store, p.id, None, "~/static/app.min.js.map", Bytes::from(MAP)).await.unwrap();
    let set = sourcemaps::load_for_event(&db.pool, &store, other.id, &event(None, URL)).await.unwrap();
    assert!(set.get(URL).is_none());
    db.cleanup().await;
}

#[tokio::test]
async fn invalid_map_is_rejected() {
    let (db, store, p) = setup().await;
    let err = sourcemaps::store_sourcemap(&db.pool, &store, p.id, None, "~/a.js.map", Bytes::from_static(b"nope")).await.unwrap_err();
    assert!(matches!(err, SourceMapError::Invalid(_)));
    db.cleanup().await;
}

#[tokio::test]
async fn uploading_same_name_and_release_replaces_the_map() {
    let (db, store, p) = setup().await;
    for _ in 0..2 {
        sourcemaps::store_sourcemap(&db.pool, &store, p.id, Some("1.0"), "~/a.js.map", Bytes::from(MAP)).await.unwrap();
    }
    assert_eq!(sourcemaps::list(&db.pool, p.id).await.unwrap().len(), 1);
    db.cleanup().await;
}

#[tokio::test]
async fn list_and_delete() {
    let (db, store, p) = setup().await;
    sourcemaps::store_sourcemap(&db.pool, &store, p.id, Some("1.0"), "~/a.js.map", Bytes::from(MAP)).await.unwrap();
    let rows = sourcemaps::list(&db.pool, p.id).await.unwrap();
    assert_eq!(rows[0].name, "~/a.js.map");
    assert_eq!(rows[0].release.as_deref(), Some("1.0"));
    sourcemaps::delete(&db.pool, &store, p.id, rows[0].id).await.unwrap();
    assert!(sourcemaps::list(&db.pool, p.id).await.unwrap().is_empty());
    db.cleanup().await;
}
