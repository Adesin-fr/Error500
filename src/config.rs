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
    /// Serveur SMTP des alertes (`SMTP_URL`, ex. `smtps://user:pass@host`) ; sans lui, pas d'email.
    pub smtp_url: Option<String>,
    pub mail_from: String,
    /// Alertes email maximum par projet et par heure (`ALERT_MAX_PER_HOUR`, 0 = illimité).
    pub alert_limit: u32,
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
            smtp_url: var("SMTP_URL").filter(|v| !v.is_empty()),
            alert_limit: match var("ALERT_MAX_PER_HOUR") {
                Some(v) => v.parse().map_err(|_| anyhow::anyhow!("ALERT_MAX_PER_HOUR invalide : {v}"))?,
                None => 10,
            },
            mail_from: var("MAIL_FROM").unwrap_or_else(|| "error500@localhost".into()),
        })
    }

    /// Transport d'alertes : SMTP si configuré, sinon désactivé.
    pub fn mailer(&self) -> anyhow::Result<crate::alerts::Mailer> {
        match &self.smtp_url {
            Some(url) => crate::alerts::Mailer::smtp(url, &self.mail_from),
            None => Ok(crate::alerts::Mailer::disabled()),
        }
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
    fn smtp_is_optional_and_from_has_a_default() {
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x")])).unwrap();
        assert_eq!(c.smtp_url, None);
        assert_eq!(c.mail_from, "error500@localhost");
        let c = Config::from_vars(vars(&[
            ("DATABASE_URL", "mysql://x"),
            ("SMTP_URL", "smtps://u:p@smtp.example.com"),
            ("MAIL_FROM", "Error500 <errors@example.com>"),
        ]))
        .unwrap();
        assert_eq!(c.smtp_url.as_deref(), Some("smtps://u:p@smtp.example.com"));
        assert_eq!(c.mail_from, "Error500 <errors@example.com>");
    }

    #[test]
    fn alert_limit_defaults_to_ten_per_hour_and_is_configurable() {
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x")])).unwrap();
        assert_eq!(c.alert_limit, 10);
        let c = Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("ALERT_MAX_PER_HOUR", "0")])).unwrap();
        assert_eq!(c.alert_limit, 0);
        assert!(Config::from_vars(vars(&[("DATABASE_URL", "mysql://x"), ("ALERT_MAX_PER_HOUR", "beaucoup")])).is_err());
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
