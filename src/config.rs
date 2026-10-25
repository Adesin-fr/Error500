//! Configuration de l'application, lue depuis l'environnement.

use crate::storage::StorageConfig;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind: String,
    /// URL publique, utilisée pour construire les DSN donnés aux SDK.
    pub public_url: String,
    pub max_body: usize,
    pub storage: StorageConfig,
    /// Serveur de développement Vite (`VITE_DEV_URL`) ; absent en production.
    pub vite_dev_url: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        Self::from_vars(|k| std::env::var(k).ok())
    }

    pub fn from_vars(var: impl Fn(&str) -> Option<String>) -> anyhow::Result<Self> {
        let database_url = var("DATABASE_URL").ok_or_else(|| anyhow::anyhow!("DATABASE_URL requise"))?;
        let max_body_mb: usize = match var("MAX_BODY_MB") {
            Some(v) => v.parse().map_err(|_| anyhow::anyhow!("MAX_BODY_MB invalide : {v}"))?,
            None => 20,
        };
        Ok(Config {
            database_url,
            bind: var("BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
            public_url: var("PUBLIC_URL").unwrap_or_else(|| "http://localhost:8080".into()),
            max_body: max_body_mb * 1024 * 1024,
            storage: StorageConfig::from_vars(&var)?,
            vite_dev_url: var("VITE_DEV_URL"),
        })
    }

    pub fn secure_cookies(&self) -> bool {
        self.public_url.starts_with("https://")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |k| pairs.iter().find(|(n, _)| *n == k).map(|(_, v)| v.to_string())
    }

    #[test]
    fn database_url_is_required() {
        assert!(Config::from_vars(vars(&[])).is_err());
    }

    #[test]
    fn defaults_are_applied() {
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://root@localhost/e500")])).unwrap();
        assert_eq!(c.database_url, "mysql://root@localhost/e500");
        assert_eq!(c.bind, "0.0.0.0:8080");
        assert_eq!(c.public_url, "http://localhost:8080");
        assert_eq!(c.max_body, 20 * 1024 * 1024);
        assert_eq!(c.storage, StorageConfig::Local { path: "./data/blobs".into() });
    }

    #[test]
    fn overrides_are_read() {
        let c = Config::from_vars(vars(&[
            ("DATABASE_URL", "mysql://x"),
            ("BIND", "127.0.0.1:9000"),
            ("PUBLIC_URL", "https://err.example.com"),
            ("MAX_BODY_MB", "5"),
            ("STORAGE", "s3"),
            ("S3_BUCKET", "b"),
            ("S3_ACCESS_KEY", "a"),
            ("S3_SECRET_KEY", "s"),
        ]))
        .unwrap();
        assert_eq!(c.bind, "127.0.0.1:9000");
        assert_eq!(c.public_url, "https://err.example.com");
        assert_eq!(c.max_body, 5 * 1024 * 1024);
        assert!(matches!(c.storage, StorageConfig::S3 { .. }));
    }

    #[test]
    fn vite_dev_url_is_optional() {
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x")])).unwrap();
        assert_eq!(c.vite_dev_url, None);
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("VITE_DEV_URL", "http://localhost:5173")])).unwrap();
        assert_eq!(c.vite_dev_url.as_deref(), Some("http://localhost:5173"));
    }

    #[test]
    fn cookies_are_secure_only_behind_https() {
        let http = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x")])).unwrap();
        assert!(!http.secure_cookies());
        let https = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("PUBLIC_URL", "https://err.example.com")])).unwrap();
        assert!(https.secure_cookies());
    }

    #[test]
    fn invalid_max_body_is_rejected() {
        assert!(Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("MAX_BODY_MB", "lots")])).is_err());
    }
}
