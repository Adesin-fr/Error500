mod common;

use common::TestDb;
use error500::projects::{self, Role};
use error500::users;

async fn user(db: &TestDb, email: &str) -> users::User {
    users::create_user(&db.pool, email, email, "correct horse").await.unwrap()
}

#[tokio::test]
async fn create_project_makes_creator_owner_and_generates_key() {
    let db = TestDb::new().await;
    let u = user(&db, "a@example.com").await;
    let p = projects::create_project(&db.pool, u.id, "My App").await.unwrap();
    assert_eq!(p.name, "My App");
    assert_eq!(p.slug, "my-app");
    assert_eq!(p.public_key.len(), 32);
    assert_eq!(projects::user_role(&db.pool, p.id, u.id).await.unwrap(), Some(Role::Owner));
    db.cleanup().await;
}

#[tokio::test]
async fn slugs_are_made_unique() {
    let db = TestDb::new().await;
    let u = user(&db, "a@example.com").await;
    let p1 = projects::create_project(&db.pool, u.id, "My App").await.unwrap();
    let p2 = projects::create_project(&db.pool, u.id, "My App").await.unwrap();
    assert_eq!(p1.slug, "my-app");
    assert_ne!(p1.slug, p2.slug);
    assert_ne!(p1.public_key, p2.public_key);
    db.cleanup().await;
}

#[tokio::test]
async fn projects_are_isolated_between_users_except_admin() {
    let db = TestDb::new().await;
    let admin = user(&db, "admin@example.com").await; // premier utilisateur => admin
    let alice = user(&db, "alice@example.com").await;
    let bob = user(&db, "bob@example.com").await;
    let pa = projects::create_project(&db.pool, alice.id, "Alice").await.unwrap();
    let pb = projects::create_project(&db.pool, bob.id, "Bob").await.unwrap();

    let ids = |v: Vec<projects::Project>| v.into_iter().map(|p| p.id).collect::<Vec<_>>();
    assert_eq!(ids(projects::list_for_user(&db.pool, &alice).await.unwrap()), vec![pa.id]);
    assert_eq!(ids(projects::list_for_user(&db.pool, &bob).await.unwrap()), vec![pb.id]);
    assert_eq!(ids(projects::list_for_user(&db.pool, &admin).await.unwrap()), vec![pa.id, pb.id]);
    db.cleanup().await;
}

#[tokio::test]
async fn members_can_be_added_and_see_the_project() {
    let db = TestDb::new().await;
    let _admin = user(&db, "admin@example.com").await;
    let alice = user(&db, "alice@example.com").await;
    let bob = user(&db, "bob@example.com").await;
    let p = projects::create_project(&db.pool, alice.id, "Alice").await.unwrap();
    projects::add_member(&db.pool, p.id, bob.id, Role::Member).await.unwrap();
    assert_eq!(projects::user_role(&db.pool, p.id, bob.id).await.unwrap(), Some(Role::Member));
    assert_eq!(projects::list_for_user(&db.pool, &bob).await.unwrap().len(), 1);
    db.cleanup().await;
}

#[tokio::test]
async fn ingest_authorization_requires_matching_id_and_key() {
    let db = TestDb::new().await;
    let u = user(&db, "a@example.com").await;
    let p = projects::create_project(&db.pool, u.id, "A").await.unwrap();
    let other = projects::create_project(&db.pool, u.id, "B").await.unwrap();
    assert!(projects::authorize_ingest(&db.pool, p.id, &p.public_key).await.unwrap().is_some());
    assert!(projects::authorize_ingest(&db.pool, p.id, &other.public_key).await.unwrap().is_none());
    assert!(projects::authorize_ingest(&db.pool, p.id, "nope").await.unwrap().is_none());
    assert!(projects::authorize_ingest(&db.pool, 9999, &p.public_key).await.unwrap().is_none());
    db.cleanup().await;
}

#[tokio::test]
async fn rotating_key_invalidates_the_old_one() {
    let db = TestDb::new().await;
    let u = user(&db, "a@example.com").await;
    let p = projects::create_project(&db.pool, u.id, "A").await.unwrap();
    let new_key = projects::rotate_key(&db.pool, p.id).await.unwrap();
    assert_ne!(new_key, p.public_key);
    assert!(projects::authorize_ingest(&db.pool, p.id, &p.public_key).await.unwrap().is_none());
    assert!(projects::authorize_ingest(&db.pool, p.id, &new_key).await.unwrap().is_some());
    db.cleanup().await;
}
