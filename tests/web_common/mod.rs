//! Harnais commun aux tests des pages web (Inertia).
#![allow(dead_code)]

use super::common::TestDb;
use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use axum::Router;
use error500::storage::BlobStore;
use error500::web::assets::Assets;
use error500::web::{router, WebState};
use http_body_util::BodyExt;
use serde_json::Value;
use std::sync::Arc;
use tower::ServiceExt;

pub struct W {
    pub db: TestDb,
    pub store: BlobStore,
    pub router: Router,
    pub version: String,
}

pub async fn web_app_with(assets: Assets) -> W {
    let db = TestDb::new().await;
    let store = BlobStore::memory();
    let version = assets.version.clone();
    let router = router(WebState {
        pool: db.pool.clone(),
        store: store.clone(),
        public_url: "https://err.example.com".into(),
        assets: Arc::new(assets),
        dev_url: None,
        secure_cookies: false,
        live_poll: std::time::Duration::from_millis(50),
    });
    W { db, store, router, version }
}

pub async fn web_app() -> W {
    web_app_with(Assets::empty()).await
}

pub struct Resp {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Resp {
    pub fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    pub fn location(&self) -> Option<String> {
        self.headers.get(header::LOCATION).map(|v| v.to_str().unwrap().to_string())
    }
    /// Valeur `name=value` du premier Set-Cookie, prête à renvoyer dans `Cookie`.
    pub fn set_cookie_pair(&self) -> Option<String> {
        let v = self.headers.get(header::SET_COOKIE)?.to_str().ok()?;
        Some(v.split(';').next()?.to_string())
    }
    pub fn set_cookie_raw(&self) -> Option<String> {
        self.headers.get(header::SET_COOKIE).map(|v| v.to_str().unwrap().to_string())
    }
    pub fn component(&self) -> String {
        self.json()["component"].as_str().unwrap_or("").to_string()
    }
    pub fn props(&self) -> Value {
        self.json()["props"].clone()
    }
}

impl W {
    pub async fn send(&self, req: Request<Body>) -> Resp {
        let resp = self.router.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
        Resp { status, headers, body }
    }

    fn base(&self, method: Method, uri: &str, cookie: Option<&str>) -> axum::http::request::Builder {
        let mut b = Request::builder()
            .method(method)
            .uri(uri)
            .header("X-Inertia", "true")
            .header("X-Inertia-Version", &self.version);
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        b
    }

    /// GET en tant que visite Inertia (réponse JSON).
    pub async fn get(&self, uri: &str, cookie: Option<&str>) -> Resp {
        self.send(self.base(Method::GET, uri, cookie).body(Body::empty()).unwrap()).await
    }

    /// GET « navigateur » : sans en-têtes Inertia (réponse HTML).
    pub async fn get_html(&self, uri: &str, cookie: Option<&str>) -> Resp {
        let mut b = Request::builder().method(Method::GET).uri(uri);
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        self.send(b.body(Body::empty()).unwrap()).await
    }

    pub async fn post_json(&self, uri: &str, cookie: Option<&str>, body: Value) -> Resp {
        self.send(
            self.base(Method::POST, uri, cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
    }

    pub async fn delete(&self, uri: &str, cookie: Option<&str>) -> Resp {
        self.send(self.base(Method::DELETE, uri, cookie).body(Body::empty()).unwrap()).await
    }

    /// GET d'un flux (SSE) : renvoie le statut, les en-têtes et le corps à lire au fil de l'eau.
    pub async fn open_stream(&self, uri: &str, cookie: Option<&str>) -> (StatusCode, HeaderMap, axum::body::BodyDataStream) {
        let mut b = Request::builder().method(Method::GET).uri(uri).header(header::ACCEPT, "text/event-stream");
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, c);
        }
        let resp = self.router.clone().oneshot(b.body(Body::empty()).unwrap()).await.unwrap();
        (resp.status(), resp.headers().clone(), resp.into_body().into_data_stream())
    }

    /// Crée un utilisateur et une session ; renvoie (id, cookie).
    pub async fn login_as(&self, email: &str) -> (u64, String) {
        let u = error500::users::create_user(&self.db.pool, email, email, "correct horse").await.unwrap();
        let token = error500::users::create_session(&self.db.pool, u.id).await.unwrap();
        (u.id, format!("e500_session={token}"))
    }
}

// --- Données de test ---

use error500::envelope::Envelope;
use error500::projects::{self, Project};
use serde_json::json;

impl W {
    pub async fn project_for(&self, owner_id: u64, name: &str) -> Project {
        projects::create_project(&self.db.pool, owner_id, name).await.unwrap()
    }

    /// Ingère un événement d'erreur (type `ty`, regroupé par `function`).
    pub async fn event(&self, p: &Project, id: &str, ty: &str, ts: f64) {
        self.event_payload(p, json!({"event_id": id, "timestamp": ts, "release": "1.0", "environment": "prod",
            "exception": {"values": [{"type": ty, "value": "boom"}]}})).await;
    }

    pub async fn event_payload(&self, p: &Project, payload: Value) {
        let ev = payload.to_string();
        let raw = format!("{{}}\n{{\"type\":\"event\",\"length\":{}}}\n{ev}\n", ev.len());
        error500::ingest::ingest_envelope(&self.db.pool, &self.store, p, &Envelope::parse(raw.as_bytes()).unwrap()).await.unwrap();
    }
}

pub fn hex_id(n: u32) -> String {
    format!("{n:032x}")
}
