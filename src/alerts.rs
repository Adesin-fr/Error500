//! Alertes email : nouvelle issue ou régression, envoyées aux membres du projet qui les ont activées.

use crate::ingest::{Alert, AlertTrigger};
use crate::projects::Project;
use lettre::message::Mailbox;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use sqlx::MySqlPool;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub body: String,
}

#[derive(Clone)]
pub enum Mailer {
    /// Pas de SMTP configuré : les alertes sont ignorées.
    Disabled,
    /// Boîte d'envoi en mémoire (tests).
    Memory(Arc<Mutex<Vec<Mail>>>),
    Smtp { transport: AsyncSmtpTransport<Tokio1Executor>, from: Mailbox },
}

impl Mailer {
    pub fn disabled() -> Self {
        Mailer::Disabled
    }

    pub fn memory() -> Self {
        Mailer::Memory(Arc::default())
    }

    /// `url` : `smtp://user:pass@host:587`, `smtps://…` (TLS) ou `smtp://host:25?tls=required`.
    pub fn smtp(url: &str, from: &str) -> anyhow::Result<Self> {
        let from: Mailbox = from.parse().map_err(|e| anyhow::anyhow!("MAIL_FROM invalide ({from}) : {e}"))?;
        let transport = AsyncSmtpTransport::<Tokio1Executor>::from_url(url)
            .map_err(|e| anyhow::anyhow!("SMTP_URL invalide : {e}"))?
            .build();
        Ok(Mailer::Smtp { transport, from })
    }

    /// Messages « envoyés » (transport mémoire uniquement).
    pub fn outbox(&self) -> Vec<Mail> {
        match self {
            Mailer::Memory(m) => m.lock().unwrap().clone(),
            _ => Vec::new(),
        }
    }

    /// Envoi synchrone, avec l'erreur du serveur SMTP le cas échéant.
    pub async fn deliver(&self, mail: Mail) -> anyhow::Result<()> {
        match self {
            Mailer::Disabled => Ok(()),
            Mailer::Memory(m) => {
                m.lock().unwrap().push(mail);
                Ok(())
            }
            Mailer::Smtp { transport, from } => {
                let message = Message::builder()
                    .from(from.clone())
                    .to(mail.to.parse()?)
                    .subject(mail.subject)
                    .body(mail.body)?;
                transport.send(message).await?;
                Ok(())
            }
        }
    }

    /// Envoi sans attendre le serveur SMTP : un SMTP lent ne doit pas ralentir l'ingestion.
    pub fn send(&self, mail: Mail) {
        match self {
            Mailer::Disabled => {}
            Mailer::Memory(m) => m.lock().unwrap().push(mail),
            Mailer::Smtp { .. } => {
                let mailer = self.clone();
                tokio::spawn(async move {
                    let to = mail.to.clone();
                    if let Err(e) = mailer.deliver(mail).await {
                        tracing::warn!("échec d'envoi de l'alerte à {to} : {e}");
                    }
                });
            }
        }
    }
}

/// Envoie l'alerte à chaque membre du projet qui reçoit les notifications.
pub async fn notify(
    pool: &MySqlPool,
    mailer: &Mailer,
    public_url: &str,
    project: &Project,
    alert: &Alert,
) -> Result<(), sqlx::Error> {
    if matches!(mailer, Mailer::Disabled) {
        return Ok(());
    }
    let issue: Option<(String, String)> = sqlx::query_as("SELECT title, level FROM issues WHERE id = ?")
        .bind(alert.issue_id)
        .fetch_optional(pool)
        .await?;
    let Some((title, level)) = issue else { return Ok(()) };
    let recipients: Vec<(String,)> = sqlx::query_as(
        "SELECT u.email FROM project_members m JOIN users u ON u.id = m.user_id \
         WHERE m.project_id = ? AND m.notify = TRUE ORDER BY u.email",
    )
    .bind(project.id)
    .fetch_all(pool)
    .await?;

    let label = match alert.trigger {
        AlertTrigger::NewIssue => "New issue",
        AlertTrigger::Regression => "Regression",
    };
    let link = format!("{}/projects/{}/issues/{}", public_url.trim_end_matches('/'), project.id, alert.issue_id);
    let subject = format!("[{}] {label}: {title}", project.name);
    let body = format!(
        "{label} in {project}\n\n{title}\nLevel: {level}\n\n{link}\n\n\
         You receive this because you are a member of {project}. \
         You can turn these emails off in the project settings.\n",
        project = project.name
    );
    for (to,) in recipients {
        mailer.send(Mail { to, subject: subject.clone(), body: body.clone() });
    }
    Ok(())
}
