mod common;

use common::TestDb;
use error500::users::{self, UserError};

#[tokio::test]
async fn first_user_is_admin_and_second_is_not() {
    let db = TestDb::new().await;
    let a = users::create_user(&db.pool, "a@example.com", "Alice", "correct horse").await.unwrap();
    let b = users::create_user(&db.pool, "b@example.com", "Bob", "correct horse").await.unwrap();
    assert!(a.is_admin);
    assert!(!b.is_admin);
    db.cleanup().await;
}

#[tokio::test]
async fn duplicate_email_is_rejected_case_insensitively() {
    let db = TestDb::new().await;
    users::create_user(&db.pool, "a@example.com", "Alice", "correct horse").await.unwrap();
    let err = users::create_user(&db.pool, "A@Example.com", "Alice2", "correct horse").await.unwrap_err();
    assert!(matches!(err, UserError::EmailTaken));
    db.cleanup().await;
}

#[tokio::test]
async fn short_password_is_rejected() {
    let db = TestDb::new().await;
    let err = users::create_user(&db.pool, "a@example.com", "Alice", "short").await.unwrap_err();
    assert!(matches!(err, UserError::WeakPassword));
    db.cleanup().await;
}

#[tokio::test]
async fn authenticate_checks_password() {
    let db = TestDb::new().await;
    users::create_user(&db.pool, "a@example.com", "Alice", "correct horse").await.unwrap();
    assert!(users::authenticate(&db.pool, "a@example.com", "correct horse").await.unwrap().is_some());
    assert!(users::authenticate(&db.pool, "a@example.com", "wrong password").await.unwrap().is_none());
    assert!(users::authenticate(&db.pool, "nobody@example.com", "correct horse").await.unwrap().is_none());
    db.cleanup().await;
}

#[tokio::test]
async fn session_roundtrip_and_logout() {
    let db = TestDb::new().await;
    let u = users::create_user(&db.pool, "a@example.com", "Alice", "correct horse").await.unwrap();
    let token = users::create_session(&db.pool, u.id).await.unwrap();
    let found = users::user_for_session(&db.pool, &token).await.unwrap().unwrap();
    assert_eq!(found.id, u.id);
    assert!(users::user_for_session(&db.pool, "bogus").await.unwrap().is_none());
    users::delete_session(&db.pool, &token).await.unwrap();
    assert!(users::user_for_session(&db.pool, &token).await.unwrap().is_none());
    db.cleanup().await;
}
