mod common;
mod web_common;

use axum::http::StatusCode;
use serde_json::json;
use web_common::web_app;

#[tokio::test]
async fn protected_pages_redirect_anonymous_visitors_to_login() {
    let w = web_app().await;
    for uri in ["/projects", "/", "/projects/1/issues", "/admin/users"] {
        let r = w.get(uri, None).await;
        assert!(r.status.is_redirection(), "{uri} -> {}", r.status);
        assert_eq!(r.location().as_deref(), Some("/login"), "{uri}");
    }
    w.db.cleanup().await;
}

#[tokio::test]
async fn first_run_sends_login_to_setup() {
    let w = web_app().await;
    let r = w.get("/login", None).await;
    assert_eq!(r.location().as_deref(), Some("/setup"));
    let r = w.get("/setup", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.component(), "Auth/Setup");
    w.db.cleanup().await;
}

#[tokio::test]
async fn setup_creates_the_admin_and_logs_in() {
    let w = web_app().await;
    let r = w.post_json("/setup", None, json!({"name": "Root", "email": "root@example.com", "password": "correct horse"})).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location().as_deref(), Some("/projects"));
    let cookie = r.set_cookie_pair().expect("cookie de session");
    let raw = r.set_cookie_raw().unwrap();
    assert!(raw.contains("HttpOnly") && raw.contains("SameSite=Lax"));

    let is_admin: bool = sqlx::query_scalar("SELECT is_admin FROM users WHERE email = 'root@example.com'").fetch_one(&w.db.pool).await.unwrap();
    assert!(is_admin);
    // le cookie est reconnu : /login renvoie vers /projects
    assert_eq!(w.get("/login", Some(&cookie)).await.location().as_deref(), Some("/projects"));
    w.db.cleanup().await;
}

#[tokio::test]
async fn setup_is_closed_once_a_user_exists() {
    let w = web_app().await;
    w.login_as("a@example.com").await;
    assert_eq!(w.get("/setup", None).await.location().as_deref(), Some("/login"));
    let r = w.post_json("/setup", None, json!({"name": "X", "email": "x@example.com", "password": "correct horse"})).await;
    assert_eq!(r.location().as_deref(), Some("/login"));
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(&w.db.pool).await.unwrap();
    assert_eq!(n, 1);
    w.db.cleanup().await;
}

#[tokio::test]
async fn setup_with_a_weak_password_shows_an_error() {
    let w = web_app().await;
    let r = w.post_json("/setup", None, json!({"name": "Root", "email": "root@example.com", "password": "short"})).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.component(), "Auth/Setup");
    assert!(r.props()["errors"]["password"].is_string());
    w.db.cleanup().await;
}

#[tokio::test]
async fn login_page_is_shown_when_users_exist() {
    let w = web_app().await;
    w.login_as("a@example.com").await;
    let r = w.get("/login", None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.component(), "Auth/Login");
    w.db.cleanup().await;
}

#[tokio::test]
async fn login_with_good_credentials_redirects_with_a_session() {
    let w = web_app().await;
    w.login_as("a@example.com").await;
    let r = w.post_json("/login", None, json!({"email": "A@Example.com", "password": "correct horse"})).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location().as_deref(), Some("/projects"));
    let cookie = r.set_cookie_pair().unwrap();
    assert_eq!(w.get("/login", Some(&cookie)).await.location().as_deref(), Some("/projects"));
    w.db.cleanup().await;
}

#[tokio::test]
async fn login_with_bad_credentials_re_renders_with_a_generic_error() {
    let w = web_app().await;
    w.login_as("a@example.com").await;
    for (email, pw) in [("a@example.com", "wrong password"), ("nobody@example.com", "correct horse")] {
        let r = w.post_json("/login", None, json!({"email": email, "password": pw})).await;
        assert_eq!(r.status, StatusCode::OK);
        assert_eq!(r.component(), "Auth/Login");
        assert!(r.props()["errors"]["email"].is_string());
        assert!(r.set_cookie_pair().is_none());
    }
    w.db.cleanup().await;
}

#[tokio::test]
async fn logout_ends_the_session() {
    let w = web_app().await;
    let (_, cookie) = w.login_as("a@example.com").await;
    let r = w.post_json("/logout", Some(&cookie), json!({})).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location().as_deref(), Some("/login"));
    assert!(r.set_cookie_raw().unwrap().contains("Max-Age=0"));
    // l'ancien cookie ne donne plus accès
    assert_eq!(w.get("/login", Some(&cookie)).await.component(), "Auth/Login");
    w.db.cleanup().await;
}

#[tokio::test]
async fn browser_first_load_gets_html_with_embedded_page_data() {
    let w = web_app().await;
    w.login_as("a@example.com").await;
    let r = w.get_html("/login", None).await;
    assert_eq!(r.status, StatusCode::OK);
    let html = r.text();
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains(r#"data-page="app""#));
    assert!(html.contains("Auth/Login"));
    w.db.cleanup().await;
}

#[tokio::test]
async fn stale_asset_version_forces_a_full_reload() {
    let w = web_app().await;
    w.login_as("a@example.com").await;
    let req = axum::http::Request::builder()
        .uri("/login")
        .header("X-Inertia", "true")
        .header("X-Inertia-Version", "old-build")
        .body(axum::body::Body::empty())
        .unwrap();
    assert_eq!(w.send(req).await.status, StatusCode::CONFLICT);
    w.db.cleanup().await;
}

#[tokio::test]
async fn built_assets_are_served_with_their_content_type() {
    let mut assets = error500::web::assets::Assets::empty();
    assets.add_file("assets/app-1.js", b"console.log(1)".to_vec());
    let w = web_common::web_app_with(assets).await;
    let ok = w.get_html("/assets/app-1.js", None).await;
    assert_eq!(ok.status, StatusCode::OK);
    assert_eq!(ok.headers["content-type"], "text/javascript");
    assert!(ok.headers["cache-control"].to_str().unwrap().contains("immutable"));
    assert_eq!(ok.body, b"console.log(1)");
    assert_eq!(w.get_html("/assets/missing.js", None).await.status, StatusCode::NOT_FOUND);
    w.db.cleanup().await;
}
