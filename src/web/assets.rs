//! Assets du frontend (build Vite embarqué dans le binaire) et gabarit HTML racine.

use rust_embed::RustEmbed;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

/// Build du frontend (`npm run build` dans `frontend/`). Peut être absent (dev, tests).
#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
#[allow_missing = true]
struct Dist;

#[derive(Debug, Clone)]
pub struct Asset {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct Assets {
    pub entry_js: Option<String>,
    pub css: Vec<String>,
    /// Version Inertia : change quand le build change, ce qui force un rechargement complet côté client.
    pub version: String,
    files: HashMap<String, Asset>,
}

fn content_type_of(path: &str) -> String {
    match path.rsplit('.').next() {
        Some("js" | "mjs") => "text/javascript".into(),
        _ => mime_guess::from_path(path).first_or_octet_stream().essence_str().to_string(),
    }
}

impl Assets {
    pub fn empty() -> Self {
        Assets { version: "dev".into(), ..Default::default() }
    }

    pub fn from_manifest(manifest: &str) -> anyhow::Result<Self> {
        let v: Value = serde_json::from_str(manifest)?;
        let entry = v
            .as_object()
            .and_then(|m| m.values().find(|e| e.get("isEntry").and_then(Value::as_bool) == Some(true)))
            .ok_or_else(|| anyhow::anyhow!("aucune entrée dans le manifeste Vite"))?;
        Ok(Assets {
            entry_js: entry.get("file").and_then(Value::as_str).map(str::to_string),
            css: entry
                .get("css")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect())
                .unwrap_or_default(),
            version: hex::encode(Sha256::digest(manifest.as_bytes()))[..16].to_string(),
            files: HashMap::new(),
        })
    }

    /// Assets compilés dans le binaire ; vides si le frontend n'a pas été construit.
    pub fn embedded() -> Self {
        let Some(manifest) = Dist::get("manifest.json") else { return Assets::empty() };
        let Ok(mut assets) = Assets::from_manifest(&String::from_utf8_lossy(&manifest.data)) else {
            return Assets::empty();
        };
        for path in Dist::iter().filter(|p| p.as_ref() != "manifest.json") {
            if let Some(f) = Dist::get(&path) {
                assets.add_file(&path, f.data.into_owned());
            }
        }
        assets
    }

    pub fn add_file(&mut self, path: &str, bytes: Vec<u8>) {
        self.files.insert(path.to_string(), Asset { content_type: content_type_of(path), bytes });
    }

    pub fn get(&self, path: &str) -> Option<&Asset> {
        if path.split('/').any(|seg| seg == "..") {
            return None;
        }
        self.files.get(path)
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Page HTML racine. `data_page` est déjà échappé pour un `<script>` (fourni par `inertia_rs`).
/// Avec `dev_url`, les scripts viennent du serveur de développement Vite.
pub fn render_shell(data_page: &str, assets: &Assets, dev_url: Option<&str>, title: &str) -> String {
    let (mut head, mut body) = (String::new(), String::new());
    match dev_url {
        Some(url) => {
            let url = url.trim_end_matches('/');
            head.push_str(&format!(r#"<script type="module" src="{url}/@vite/client"></script>"#));
            body.push_str(&format!(r#"<script type="module" src="{url}/src/main.ts"></script>"#));
        }
        None => {
            for css in &assets.css {
                head.push_str(&format!(r#"<link rel="stylesheet" href="/{css}">"#));
            }
            if let Some(js) = &assets.entry_js {
                body.push_str(&format!(r#"<script type="module" src="/{js}"></script>"#));
            }
        }
    }
    format!(
        concat!(
            "<!DOCTYPE html><html lang=\"fr\"><head><meta charset=\"utf-8\">",
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">",
            "<title>{title}</title>{head}</head><body>",
            "<script data-page=\"app\" type=\"application/json\">{data}</script>",
            "<div id=\"app\"></div>{body}</body></html>"
        ),
        title = escape_html(title),
        head = head,
        data = data_page,
        body = body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"{
        "_shared-abc.js": {"file": "assets/shared-abc.js"},
        "src/main.ts": {"file": "assets/main-123.js", "src": "src/main.ts", "isEntry": true,
                        "css": ["assets/main-456.css"], "imports": ["_shared-abc.js"]}
    }"#;

    #[test]
    fn manifest_entry_and_css_are_found() {
        let a = Assets::from_manifest(MANIFEST).unwrap();
        assert_eq!(a.entry_js.as_deref(), Some("assets/main-123.js"));
        assert_eq!(a.css, vec!["assets/main-456.css"]);
    }

    #[test]
    fn version_changes_with_the_manifest() {
        let a = Assets::from_manifest(MANIFEST).unwrap();
        let b = Assets::from_manifest(&MANIFEST.replace("123", "999")).unwrap();
        assert_ne!(a.version, b.version);
        assert_eq!(a.version, Assets::from_manifest(MANIFEST).unwrap().version);
    }

    #[test]
    fn invalid_manifest_is_an_error() {
        assert!(Assets::from_manifest("nope").is_err());
    }

    #[test]
    fn empty_assets_have_a_stable_version() {
        assert_eq!(Assets::empty().version, Assets::empty().version);
        assert!(Assets::empty().entry_js.is_none());
    }

    #[test]
    fn shell_links_built_assets() {
        let a = Assets::from_manifest(MANIFEST).unwrap();
        let html = render_shell(r#"{"component":"X"}"#, &a, None, "error500");
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains(r#"<script type="module" src="/assets/main-123.js"></script>"#));
        assert!(html.contains(r#"<link rel="stylesheet" href="/assets/main-456.css">"#));
        assert!(html.contains(r#"<script data-page="app" type="application/json">{"component":"X"}</script>"#));
        assert!(html.contains(r#"<div id="app"></div>"#));
    }

    #[test]
    fn shell_in_dev_mode_points_to_the_vite_server() {
        let html = render_shell("{}", &Assets::empty(), Some("http://localhost:5173/"), "error500");
        assert!(html.contains(r#"src="http://localhost:5173/@vite/client""#));
        assert!(html.contains(r#"src="http://localhost:5173/src/main.ts""#));
    }

    #[test]
    fn shell_escapes_the_title() {
        let html = render_shell("{}", &Assets::empty(), None, "<b>&");
        assert!(html.contains("<title>&lt;b&gt;&amp;</title>"));
    }

    #[test]
    fn extra_files_are_served_with_their_content_type() {
        let mut a = Assets::empty();
        a.add_file("assets/app.js", b"console.log(1)".to_vec());
        a.add_file("assets/app.css", b"a{}".to_vec());
        let js = a.get("assets/app.js").unwrap();
        assert_eq!(js.content_type, "text/javascript");
        assert_eq!(js.bytes, b"console.log(1)");
        assert_eq!(a.get("assets/app.css").unwrap().content_type, "text/css");
        assert!(a.get("assets/missing.js").is_none());
        assert!(a.get("../secret").is_none());
    }
}
