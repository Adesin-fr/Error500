mod common;
mod web_common;

use axum::http::StatusCode;
use error500::{totp, users};
use serde_json::{json, Value};
use web_common::{web_app, Resp, W};

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
}

/// Active la 2FA pour l'utilisateur connecté ; renvoie (secret, codes de secours).
async fn enable_2fa(w: &W, cookie: &str) -> (String, Vec<String>) {
    let r = w.post_json("/account/2fa/setup", Some(cookie), json!({})).await;
    let secret = r.props()["two_factor"]["secret"].as_str().unwrap().to_string();
    let code = totp::code_at(&secret, now()).unwrap();
    let r = w.post_json("/account/2fa/enable", Some(cookie), json!({"code": code})).await;
    let codes = r.props()["recovery_codes"].as_array().unwrap().iter().map(|c| c.as_str().unwrap().to_string()).collect();
    (secret, codes)
}

async fn password_login(w: &W, email: &str) -> Resp {
    w.post_json("/login", None, json!({"email": email, "password": "correct horse"})).await
}

#[tokio::test]
async fn account_page_requires_login_and_starts_without_2fa() {
    let w = web_app().await;
    assert_eq!(w.get("/account", None).await.location().as_deref(), Some("/login"));
    let (_, cookie) = w.login_as("a@example.com").await;
    let r = w.get("/account", Some(&cookie)).await;
    assert_eq!(r.component(), "Account/Show");
    assert_eq!(r.props()["two_factor"]["enabled"], false);
    assert!(r.props()["two_factor"]["secret"].is_null());
    w.db.cleanup().await;
}

#[tokio::test]
async fn setup_shows_secret_uri_and_qr_but_does_not_enable() {
    let w = web_app().await;
    let (_, cookie) = w.login_as("a@example.com").await;
    let r = w.post_json("/account/2fa/setup", Some(&cookie), json!({})).await;
    assert_eq!(r.component(), "Account/Show");
    let tf = &r.props()["two_factor"];
    assert_eq!(tf["enabled"], false);
    let secret = tf["secret"].as_str().unwrap();
    assert!(tf["otpauth_uri"].as_str().unwrap().contains(&format!("secret={secret}")));
    assert!(tf["qr_svg"].as_str().unwrap().starts_with("<?xml"));
    // pas encore exigée à la connexion
    assert_eq!(password_login(&w, "a@example.com").await.location().as_deref(), Some("/projects"));
    w.db.cleanup().await;
}

#[tokio::test]
async fn enable_needs_a_valid_code_then_returns_recovery_codes() {
    let w = web_app().await;
    let (uid, cookie) = w.login_as("a@example.com").await;
    let r = w.post_json("/account/2fa/setup", Some(&cookie), json!({})).await;
    let secret = r.props()["two_factor"]["secret"].as_str().unwrap().to_string();

    let r = w.post_json("/account/2fa/enable", Some(&cookie), json!({"code": "000000"})).await;
    assert!(r.props()["errors"]["code"].is_string());
    assert_eq!(r.props()["two_factor"]["enabled"], false);

    let code = totp::code_at(&secret, now()).unwrap();
    let r = w.post_json("/account/2fa/enable", Some(&cookie), json!({"code": code})).await;
    assert_eq!(r.props()["two_factor"]["enabled"], true);
    assert_eq!(r.props()["recovery_codes"].as_array().unwrap().len(), 8);
    // le secret n'est plus affiché une fois la 2FA active, et les codes ne sont montrés qu'une fois
    assert!(r.props()["two_factor"]["secret"].is_null());
    let again = w.get("/account", Some(&cookie)).await;
    assert!(again.props()["recovery_codes"].is_null());
    assert_eq!(again.props()["two_factor"]["recovery_remaining"], 8);
    assert!(users::two_factor_enabled(&w.db.pool, uid).await.unwrap());
    w.db.cleanup().await;
}

#[tokio::test]
async fn login_with_2fa_asks_for_a_code_and_sets_no_session_yet() {
    let w = web_app().await;
    let (_, cookie) = w.login_as("a@example.com").await;
    enable_2fa(&w, &cookie).await;

    let r = password_login(&w, "a@example.com").await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.component(), "Auth/TwoFactor");
    assert!(r.props()["challenge"].as_str().unwrap().len() >= 32);
    assert!(r.set_cookie_pair().is_none());
    w.db.cleanup().await;
}

#[tokio::test]
async fn second_step_refuses_the_code_used_to_enable() {
    let w = web_app().await;
    let (_, cookie) = w.login_as("a@example.com").await;
    let (secret, _) = enable_2fa(&w, &cookie).await;

    let challenge = password_login(&w, "a@example.com").await.props()["challenge"].as_str().unwrap().to_string();
    let code = totp::code_at(&secret, now()).unwrap();
    let r = w.post_json("/login/two-factor", None, json!({"challenge": challenge, "code": code})).await;
    // même période que l'activation → refusé (rejeu)
    assert_eq!(r.component(), "Auth/TwoFactor");
    assert!(r.set_cookie_pair().is_none());
    w.db.cleanup().await;
}

#[tokio::test]
async fn second_step_accepts_a_fresh_code_and_refuses_replays() {
    let w = web_app().await;
    let (uid, cookie) = w.login_as("a@example.com").await;
    let (secret, _) = enable_2fa(&w, &cookie).await;
    // simule le dernier code utilisé il y a deux périodes
    sqlx::query("UPDATE users SET totp_last_step = ? WHERE id = ?")
        .bind((now() / 30 - 2) as i64)
        .bind(uid)
        .execute(&w.db.pool)
        .await
        .unwrap();

    let challenge = password_login(&w, "a@example.com").await.props()["challenge"].as_str().unwrap().to_string();
    let code = totp::code_at(&secret, now()).unwrap();
    let r = w.post_json("/login/two-factor", None, json!({"challenge": challenge, "code": code})).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.location().as_deref(), Some("/projects"));
    let session = r.set_cookie_pair().expect("cookie de session");
    assert_eq!(w.get("/login", Some(&session)).await.location().as_deref(), Some("/projects"));

    // même code sur un nouveau défi : rejeu refusé
    let challenge = password_login(&w, "a@example.com").await.props()["challenge"].as_str().unwrap().to_string();
    let r = w.post_json("/login/two-factor", None, json!({"challenge": challenge, "code": code})).await;
    assert!(r.set_cookie_pair().is_none());
    // le défi consommé ne sert plus
    let r = w.post_json("/login/two-factor", None, json!({"challenge": "inconnu", "code": code})).await;
    assert_eq!(r.component(), "Auth/Login");
    w.db.cleanup().await;
}

#[tokio::test]
async fn a_recovery_code_works_once() {
    let w = web_app().await;
    let (_, cookie) = w.login_as("a@example.com").await;
    let (_, codes) = enable_2fa(&w, &cookie).await;

    let challenge = password_login(&w, "a@example.com").await.props()["challenge"].as_str().unwrap().to_string();
    let r = w.post_json("/login/two-factor", None, json!({"challenge": challenge, "code": codes[0]})).await;
    assert_eq!(r.location().as_deref(), Some("/projects"));

    let challenge = password_login(&w, "a@example.com").await.props()["challenge"].as_str().unwrap().to_string();
    let r = w.post_json("/login/two-factor", None, json!({"challenge": challenge, "code": codes[0]})).await;
    assert!(r.set_cookie_pair().is_none());
    assert_eq!(w.get("/account", Some(&cookie)).await.props()["two_factor"]["recovery_remaining"], 7);
    w.db.cleanup().await;
}

#[tokio::test]
async fn too_many_wrong_codes_burn_the_challenge() {
    let w = web_app().await;
    let (uid, cookie) = w.login_as("a@example.com").await;
    let (secret, _) = enable_2fa(&w, &cookie).await;
    sqlx::query("UPDATE users SET totp_last_step = NULL WHERE id = ?").bind(uid).execute(&w.db.pool).await.unwrap();

    let challenge = password_login(&w, "a@example.com").await.props()["challenge"].as_str().unwrap().to_string();
    for _ in 0..5 {
        let r = w.post_json("/login/two-factor", None, json!({"challenge": challenge, "code": "000000"})).await;
        assert!(r.set_cookie_pair().is_none());
    }
    // le bon code ne rattrape plus : le défi est détruit, il faut ressaisir le mot de passe
    let good = totp::code_at(&secret, now()).unwrap();
    let r = w.post_json("/login/two-factor", None, json!({"challenge": challenge, "code": good})).await;
    assert_eq!(r.component(), "Auth/Login");
    assert!(r.set_cookie_pair().is_none());
    w.db.cleanup().await;
}

#[tokio::test]
async fn wrong_password_never_reveals_that_2fa_is_enabled() {
    let w = web_app().await;
    let (_, cookie) = w.login_as("a@example.com").await;
    enable_2fa(&w, &cookie).await;
    let r = w.post_json("/login", None, json!({"email": "a@example.com", "password": "wrong password"})).await;
    assert_eq!(r.component(), "Auth/Login");
    assert!(r.props()["errors"]["email"].is_string());
    w.db.cleanup().await;
}

#[tokio::test]
async fn disabling_needs_the_password() {
    let w = web_app().await;
    let (uid, cookie) = w.login_as("a@example.com").await;
    enable_2fa(&w, &cookie).await;

    let r = w.post_json("/account/2fa/disable", Some(&cookie), json!({"password": "wrong password"})).await;
    assert!(r.props()["errors"]["password"].is_string());
    assert!(users::two_factor_enabled(&w.db.pool, uid).await.unwrap());

    let r = w.post_json("/account/2fa/disable", Some(&cookie), json!({"password": "correct horse"})).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert!(!users::two_factor_enabled(&w.db.pool, uid).await.unwrap());
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recovery_codes WHERE user_id = ?").bind(uid).fetch_one(&w.db.pool).await.unwrap();
    assert_eq!(n, 0);
    assert_eq!(password_login(&w, "a@example.com").await.location().as_deref(), Some("/projects"));
    w.db.cleanup().await;
}

#[tokio::test]
async fn account_actions_require_login() {
    let w = web_app().await;
    for uri in ["/account/2fa/setup", "/account/2fa/enable", "/account/2fa/disable"] {
        let r = w.post_json(uri, None, Value::Null).await;
        assert!(r.status.is_redirection(), "{uri}");
    }
    w.db.cleanup().await;
}

const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

#[tokio::test]
async fn the_totp_secret_is_encrypted_at_rest() {
    let w = web_app().await;
    let (uid, cookie) = w.login_as("a@example.com").await;
    let r = w.post_json("/account/2fa/setup", Some(&cookie), json!({})).await;
    let secret = r.props()["two_factor"]["secret"].as_str().unwrap().to_string();
    let stored: String = sqlx::query_scalar("SELECT totp_secret FROM users WHERE id = ?").bind(uid).fetch_one(&w.db.pool).await.unwrap();
    assert!(!stored.contains(&secret));
    let key = error500::secretbox::SecretKey::from_hex(KEY).unwrap();
    assert_eq!(key.decrypt(&stored).as_deref(), Some(secret.as_str()));
    w.db.cleanup().await;
}

#[tokio::test]
async fn without_a_secret_key_2fa_cannot_be_set_up() {
    let w = web_common::web_app_with_key(None).await;
    let (uid, cookie) = w.login_as("a@example.com").await;
    let r = w.get("/account", Some(&cookie)).await;
    assert_eq!(r.props()["two_factor"]["available"], false);
    let r = w.post_json("/account/2fa/setup", Some(&cookie), json!({})).await;
    assert!(r.props()["two_factor"]["secret"].is_null());
    assert!(r.props()["errors"]["setup"].is_string());
    let stored: Option<String> = sqlx::query_scalar("SELECT totp_secret FROM users WHERE id = ?").bind(uid).fetch_one(&w.db.pool).await.unwrap();
    assert!(stored.is_none());
    w.db.cleanup().await;
}

#[tokio::test]
async fn login_fails_closed_when_2fa_is_on_but_the_key_is_gone() {
    let mut w = web_app().await;
    let (uid, cookie) = w.login_as("a@example.com").await;
    enable_2fa(&w, &cookie).await;
    // même base, application relancée sans clé
    w.swap_key(None);
    let r = w.post_json("/login", None, json!({"email": "a@example.com", "password": "correct horse"})).await;
    assert_eq!(r.component(), "Auth/Login");
    assert!(r.set_cookie_pair().is_none());
    assert!(r.props()["errors"]["email"].as_str().unwrap().contains("SECRET_KEY"));
    let _ = uid;
    w.db.cleanup().await;
}
