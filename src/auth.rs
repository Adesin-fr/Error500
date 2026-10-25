//! Extraction de la clé publique de projet depuis une requête d'ingestion Sentry.

use url::Url;

/// `X-Sentry-Auth: Sentry sentry_version=7, sentry_key=abc, ...`
pub fn key_from_auth_header(header: &str) -> Option<String> {
    let rest = header.trim().strip_prefix("Sentry").unwrap_or(header);
    rest.split(',')
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == "sentry_key")
        .map(|(_, v)| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Chaîne de requête `?sentry_key=abc&...` (sans le `?`).
pub fn key_from_query(query: &str) -> Option<String> {
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(k, _)| k == "sentry_key")
        .map(|(_, v)| v.into_owned())
        .filter(|v| !v.is_empty())
}

/// Clé publique contenue dans un DSN (`https://key@host/id`).
pub fn key_from_dsn(dsn: &str) -> Option<String> {
    let url = Url::parse(dsn).ok()?;
    Some(url.username().to_string()).filter(|u| !u.is_empty())
}

/// DSN à donner aux SDK pour un projet.
pub fn build_dsn(base_url: &str, public_key: &str, project_id: u64) -> String {
    let base = Url::parse(base_url).expect("base_url invalide");
    let host = base.host_str().expect("base_url sans hôte");
    let port = base.port().map(|p| format!(":{p}")).unwrap_or_default();
    format!("{}://{public_key}@{host}{port}/{project_id}", base.scheme())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_from_x_sentry_auth_header() {
        let h = "Sentry sentry_version=7, sentry_client=sentry.javascript.browser/8.0, sentry_key=abc123, sentry_secret=zzz";
        assert_eq!(key_from_auth_header(h).as_deref(), Some("abc123"));
    }

    #[test]
    fn key_from_auth_header_missing() {
        assert_eq!(key_from_auth_header("Sentry sentry_version=7"), None);
        assert_eq!(key_from_auth_header(""), None);
    }

    #[test]
    fn key_from_query_string() {
        assert_eq!(key_from_query("sentry_version=7&sentry_key=k1&x=y").as_deref(), Some("k1"));
        assert_eq!(key_from_query("sentry_version=7"), None);
    }

    #[test]
    fn extracts_key_from_dsn() {
        assert_eq!(key_from_dsn("https://pubkey@o1.example.com/42").as_deref(), Some("pubkey"));
        assert_eq!(key_from_dsn("https://pubkey:secret@example.com/42").as_deref(), Some("pubkey"));
        assert_eq!(key_from_dsn("not a dsn"), None);
    }

    #[test]
    fn dsn_is_built_from_base_url() {
        assert_eq!(build_dsn("https://err.example.com/", "pk", 7), "https://pk@err.example.com/7");
        assert_eq!(build_dsn("http://localhost:8080", "pk", 1), "http://pk@localhost:8080/1");
    }
}
