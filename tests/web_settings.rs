mod common;
mod web_common;

use axum::http::StatusCode;
use bytes::Bytes;
use error500::projects::{self, Role};
use serde_json::json;
use web_common::{web_app, W};

const MAP: &str = r#"{"version":3,"sources":["a.ts"],"names":[],"mappings":"AAAA"}"#;

/// (projet, cookie propriétaire alice) ; l'admin est créé en premier.
async fn setup(w: &W) -> (projects::Project, String) {
    w.login_as("admin@example.com").await;
    let (alice, cookie) = w.login_as("alice@example.com").await;
    (w.project_for(alice, "App").await, cookie)
}

#[tokio::test]
async fn settings_page_shows_dsn_tokens_members_and_sourcemaps() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    error500::sourcemaps::store_sourcemap(&w.db.pool, &w.store, p.id, Some("1.0"), "~/a.js.map", Bytes::from(MAP)).await.unwrap();
    let r = w.get(&format!("/projects/{}/settings", p.id), Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.component(), "Projects/Settings");
    let props = r.props();
    assert_eq!(props["dsn"], format!("https://{}@err.example.com/{}", p.public_key, p.id));
    assert_eq!(props["upload_url"], format!("https://err.example.com/api/{}/sourcemaps/", p.id));
    assert_eq!(props["upload_token"].as_str().unwrap().len(), 32);
    assert_eq!(props["can_manage"], true);
    assert_eq!(props["members"][0]["email"], "alice@example.com");
    assert_eq!(props["members"][0]["role"], "owner");
    assert_eq!(props["sourcemaps"][0]["name"], "~/a.js.map");
    assert_eq!(props["sourcemaps"][0]["release"], "1.0");
    w.db.cleanup().await;
}

#[tokio::test]
async fn plain_members_see_the_dsn_but_not_the_upload_token() {
    let w = web_app().await;
    let (p, _) = setup(&w).await;
    let (bob, bob_cookie) = w.login_as("bob@example.com").await;
    projects::add_member(&w.db.pool, p.id, bob, Role::Member).await.unwrap();
    let r = w.get(&format!("/projects/{}/settings", p.id), Some(&bob_cookie)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.props()["can_manage"], false);
    assert!(r.props()["upload_token"].is_null());
    assert!(r.props()["dsn"].is_string());
    w.db.cleanup().await;
}

#[tokio::test]
async fn owner_adds_a_member_by_email() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    let (bob, _) = w.login_as("bob@example.com").await;
    let r = w.post_json(&format!("/projects/{}/members", p.id), Some(&cookie), json!({"email": "Bob@Example.com", "role": "member"})).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location(), Some(format!("/projects/{}/settings", p.id)));
    assert_eq!(projects::user_role(&w.db.pool, p.id, bob).await.unwrap(), Some(Role::Member));
    w.db.cleanup().await;
}

#[tokio::test]
async fn adding_an_unknown_email_or_bad_role_shows_an_error() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    let url = format!("/projects/{}/members", p.id);
    let r = w.post_json(&url, Some(&cookie), json!({"email": "ghost@example.com", "role": "member"})).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.component(), "Projects/Settings");
    assert!(r.props()["errors"]["email"].is_string());
    let r = w.post_json(&url, Some(&cookie), json!({"email": "admin@example.com", "role": "emperor"})).await;
    assert!(r.props()["errors"]["role"].is_string());
    w.db.cleanup().await;
}

#[tokio::test]
async fn plain_members_cannot_manage_the_project() {
    let w = web_app().await;
    let (p, _) = setup(&w).await;
    let (bob, bob_cookie) = w.login_as("bob@example.com").await;
    projects::add_member(&w.db.pool, p.id, bob, Role::Member).await.unwrap();
    let base = format!("/projects/{}", p.id);
    let attempts = [
        w.post_json(&format!("{base}/members"), Some(&bob_cookie), json!({"email": "admin@example.com", "role": "member"})).await,
        w.post_json(&format!("{base}/rotate-key"), Some(&bob_cookie), json!({})).await,
        w.post_json(&format!("{base}/rotate-upload-token"), Some(&bob_cookie), json!({})).await,
        w.delete(&format!("{base}/sourcemaps/1"), Some(&bob_cookie)).await,
        w.delete(&format!("{base}/members/{bob}"), Some(&bob_cookie)).await,
    ];
    for r in attempts {
        assert_eq!(r.status, StatusCode::FORBIDDEN);
    }
    w.db.cleanup().await;
}

#[tokio::test]
async fn removing_a_member_and_protecting_the_last_owner() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    let (alice,): (u64,) = sqlx::query_as("SELECT id FROM users WHERE email = 'alice@example.com'").fetch_one(&w.db.pool).await.unwrap();
    let (bob, _) = w.login_as("bob@example.com").await;
    projects::add_member(&w.db.pool, p.id, bob, Role::Member).await.unwrap();

    let r = w.delete(&format!("/projects/{}/members/{bob}", p.id), Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(projects::user_role(&w.db.pool, p.id, bob).await.unwrap(), None);

    let r = w.delete(&format!("/projects/{}/members/{alice}", p.id), Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::UNPROCESSABLE_ENTITY, "le dernier propriétaire ne peut pas partir");
    assert_eq!(projects::user_role(&w.db.pool, p.id, alice).await.unwrap(), Some(Role::Owner));
    w.db.cleanup().await;
}

#[tokio::test]
async fn rotating_keys_changes_them() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    let old_token = projects::upload_token(&w.db.pool, p.id).await.unwrap();
    let r = w.post_json(&format!("/projects/{}/rotate-key", p.id), Some(&cookie), json!({})).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    let now = projects::get(&w.db.pool, p.id).await.unwrap().unwrap();
    assert_ne!(now.public_key, p.public_key);

    let r = w.post_json(&format!("/projects/{}/rotate-upload-token", p.id), Some(&cookie), json!({})).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_ne!(projects::upload_token(&w.db.pool, p.id).await.unwrap(), old_token);
    w.db.cleanup().await;
}

#[tokio::test]
async fn sourcemap_can_be_deleted_by_its_owner_only_within_the_project() {
    let w = web_app().await;
    let (p, cookie) = setup(&w).await;
    let (alice,): (u64,) = sqlx::query_as("SELECT id FROM users WHERE email = 'alice@example.com'").fetch_one(&w.db.pool).await.unwrap();
    let other = w.project_for(alice, "Other").await;
    error500::sourcemaps::store_sourcemap(&w.db.pool, &w.store, p.id, None, "~/a.js.map", Bytes::from(MAP)).await.unwrap();
    let id = error500::sourcemaps::list(&w.db.pool, p.id).await.unwrap()[0].id;

    // via l'autre projet : sans effet
    w.delete(&format!("/projects/{}/sourcemaps/{id}", other.id), Some(&cookie)).await;
    assert_eq!(error500::sourcemaps::list(&w.db.pool, p.id).await.unwrap().len(), 1);

    let r = w.delete(&format!("/projects/{}/sourcemaps/{id}", p.id), Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert!(error500::sourcemaps::list(&w.db.pool, p.id).await.unwrap().is_empty());
    w.db.cleanup().await;
}

// --- Administration des utilisateurs ---

#[tokio::test]
async fn admin_lists_and_creates_users() {
    let w = web_app().await;
    let (_, admin_cookie) = w.login_as("admin@example.com").await;
    let r = w.get("/admin/users", Some(&admin_cookie)).await;
    assert_eq!(r.component(), "Admin/Users");
    assert_eq!(r.props()["users"].as_array().unwrap().len(), 1);
    assert_eq!(r.props()["users"][0]["is_admin"], true);

    let r = w.post_json("/admin/users", Some(&admin_cookie),
        json!({"name": "Eve", "email": "eve@example.com", "password": "correct horse", "is_admin": false})).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location().as_deref(), Some("/admin/users"));
    assert!(error500::users::authenticate(&w.db.pool, "eve@example.com", "correct horse").await.unwrap().is_some());

    let r = w.post_json("/admin/users", Some(&admin_cookie),
        json!({"name": "Eve2", "email": "EVE@example.com", "password": "correct horse"})).await;
    assert_eq!(r.component(), "Admin/Users");
    assert!(r.props()["errors"]["email"].is_string());
    let r = w.post_json("/admin/users", Some(&admin_cookie), json!({"name": "X", "email": "x@example.com", "password": "short"})).await;
    assert!(r.props()["errors"]["password"].is_string());
    w.db.cleanup().await;
}

#[tokio::test]
async fn admin_pages_are_hidden_from_regular_users() {
    let w = web_app().await;
    w.login_as("admin@example.com").await;
    let (_, cookie) = w.login_as("alice@example.com").await;
    assert_eq!(w.get("/admin/users", Some(&cookie)).await.status, StatusCode::NOT_FOUND);
    let r = w.post_json("/admin/users", Some(&cookie), json!({"name": "X", "email": "x@example.com", "password": "correct horse"})).await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&w.db.pool).await.unwrap();
    assert_eq!(n, 2);
    w.db.cleanup().await;
}
