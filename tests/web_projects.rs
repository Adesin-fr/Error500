mod common;
mod web_common;

use axum::http::StatusCode;
use serde_json::json;
use web_common::{hex_id, web_app};

#[tokio::test]
async fn projects_index_lists_only_my_projects_with_unresolved_counts() {
    let w = web_app().await;
    let (_admin, _) = w.login_as("admin@example.com").await;
    let (alice, cookie) = w.login_as("alice@example.com").await;
    let (bob, _) = w.login_as("bob@example.com").await;
    let mine = w.project_for(alice, "Alice App").await;
    w.project_for(bob, "Bob App").await;
    w.event(&mine, &hex_id(1), "E1", 1_700_000_000.0).await;
    w.event(&mine, &hex_id(2), "E2", 1_700_000_001.0).await;

    let r = w.get("/projects", Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.component(), "Projects/Index");
    let projects = r.props()["projects"].as_array().unwrap().clone();
    assert_eq!(projects.len(), 1);
    assert_eq!(projects[0]["name"], "Alice App");
    assert_eq!(projects[0]["unresolved_count"], 2);
    assert_eq!(r.props()["auth"]["user"]["email"], "alice@example.com");
    w.db.cleanup().await;
}

#[tokio::test]
async fn admin_sees_every_project() {
    let w = web_app().await;
    let (admin, cookie) = w.login_as("admin@example.com").await;
    let (bob, _) = w.login_as("bob@example.com").await;
    w.project_for(bob, "Bob App").await;
    w.project_for(admin, "Admin App").await;
    let r = w.get("/projects", Some(&cookie)).await;
    assert_eq!(r.props()["projects"].as_array().unwrap().len(), 2);
    w.db.cleanup().await;
}

#[tokio::test]
async fn creating_a_project_makes_me_owner_and_opens_its_issues() {
    let w = web_app().await;
    let (_admin, _) = w.login_as("admin@example.com").await;
    let (alice, cookie) = w.login_as("alice@example.com").await;
    let r = w.post_json("/projects", Some(&cookie), json!({"name": "Shiny"})).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    let loc = r.location().unwrap();
    assert!(loc.starts_with("/projects/") && loc.ends_with("/issues"), "{loc}");
    let id: u64 = loc.split('/').nth(2).unwrap().parse().unwrap();
    let role = error500::projects::user_role(&w.db.pool, id, alice).await.unwrap();
    assert_eq!(role, Some(error500::projects::Role::Owner));
    w.db.cleanup().await;
}

#[tokio::test]
async fn creating_a_project_requires_a_name() {
    let w = web_app().await;
    let (_, cookie) = w.login_as("alice@example.com").await;
    let r = w.post_json("/projects", Some(&cookie), json!({"name": "   "})).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.component(), "Projects/Index");
    assert!(r.props()["errors"]["name"].is_string());
    w.db.cleanup().await;
}

#[tokio::test]
async fn project_pages_are_hidden_from_non_members() {
    let w = web_app().await;
    let (_admin, _) = w.login_as("admin@example.com").await;
    let (alice, _) = w.login_as("alice@example.com").await;
    let (_, bob_cookie) = w.login_as("bob@example.com").await;
    let p = w.project_for(alice, "Alice App").await;
    for path in ["issues", "replays", "logs", "settings"] {
        let r = w.get(&format!("/projects/{}/{path}", p.id), Some(&bob_cookie)).await;
        assert_eq!(r.status, StatusCode::NOT_FOUND, "{path}");
    }
    assert_eq!(w.get("/projects/99999/issues", Some(&bob_cookie)).await.status, StatusCode::NOT_FOUND);
    w.db.cleanup().await;
}

#[tokio::test]
async fn project_root_redirects_to_issues() {
    let w = web_app().await;
    let (_admin, _) = w.login_as("admin@example.com").await;
    let (alice, cookie) = w.login_as("alice@example.com").await;
    let p = w.project_for(alice, "A").await;
    let r = w.get(&format!("/projects/{}", p.id), Some(&cookie)).await;
    assert_eq!(r.location(), Some(format!("/projects/{}/issues", p.id)));
    w.db.cleanup().await;
}

#[tokio::test]
async fn issues_page_lists_issues_for_the_selected_status() {
    let w = web_app().await;
    let (_admin, _) = w.login_as("admin@example.com").await;
    let (alice, cookie) = w.login_as("alice@example.com").await;
    let p = w.project_for(alice, "A").await;
    w.event(&p, &hex_id(1), "First", 1_700_000_000.0).await;
    w.event(&p, &hex_id(2), "Second", 1_700_000_100.0).await;
    let issues = error500::queries::list_issues(&w.db.pool, p.id, None, 10, 0).await.unwrap();
    error500::queries::set_issue_status(&w.db.pool, p.id, issues[1].id, error500::queries::IssueStatus::Resolved).await.unwrap();

    let r = w.get(&format!("/projects/{}/issues", p.id), Some(&cookie)).await;
    assert_eq!(r.component(), "Issues/Index");
    let props = r.props();
    assert_eq!(props["project"]["name"], "A");
    assert_eq!(props["status"], "unresolved");
    assert_eq!(props["issues"].as_array().unwrap().len(), 1);
    assert_eq!(props["counts"], json!({"unresolved": 1, "resolved": 1, "ignored": 0}));

    let r = w.get(&format!("/projects/{}/issues?status=resolved", p.id), Some(&cookie)).await;
    assert_eq!(r.props()["status"], "resolved");
    assert_eq!(r.props()["issues"].as_array().unwrap().len(), 1);
    // statut inconnu : retombe sur la valeur par défaut
    let r = w.get(&format!("/projects/{}/issues?status=bogus", p.id), Some(&cookie)).await;
    assert_eq!(r.props()["status"], "unresolved");
    w.db.cleanup().await;
}

#[tokio::test]
async fn issues_are_paginated() {
    let w = web_app().await;
    let (_admin, _) = w.login_as("admin@example.com").await;
    let (alice, cookie) = w.login_as("alice@example.com").await;
    let p = w.project_for(alice, "A").await;
    for n in 0..27u32 {
        w.event(&p, &hex_id(n + 1), &format!("Err{n}"), 1_700_000_000.0 + f64::from(n)).await;
    }
    let r = w.get(&format!("/projects/{}/issues", p.id), Some(&cookie)).await;
    assert_eq!(r.props()["issues"].as_array().unwrap().len(), 25);
    assert_eq!(r.props()["pages"], 2);
    assert_eq!(r.props()["page"], 1);
    let r = w.get(&format!("/projects/{}/issues?page=2", p.id), Some(&cookie)).await;
    assert_eq!(r.props()["issues"].as_array().unwrap().len(), 2);
    assert_eq!(r.props()["page"], 2);
    // page hors bornes : ramenée à la dernière
    let r = w.get(&format!("/projects/{}/issues?page=99", p.id), Some(&cookie)).await;
    assert_eq!(r.props()["page"], 2);
    w.db.cleanup().await;
}
