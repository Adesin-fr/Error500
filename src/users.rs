//! Utilisateurs, mots de passe (argon2) et sessions.

use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use argon2::Argon2;
use crate::secretbox::SecretKey;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, MySqlPool};

const MIN_PASSWORD_LEN: usize = 8;
const SESSION_DAYS: i64 = 30;

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("adresse e-mail déjà utilisée")]
    EmailTaken,
    #[error("mot de passe trop court (8 caractères minimum)")]
    WeakPassword,
    #[error("erreur de hachage : {0}")]
    Hash(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
}

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: u64,
    pub email: String,
    pub name: String,
    pub is_admin: bool,
}

pub async fn create_user(
    pool: &MySqlPool,
    email: &str,
    name: &str,
    password: &str,
) -> Result<User, UserError> {
    if password.chars().count() < MIN_PASSWORD_LEN {
        return Err(UserError::WeakPassword);
    }
    let email = email.trim().to_lowercase();
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| UserError::Hash(e.to_string()))?
        .to_string();

    let mut tx = pool.begin().await?;
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users FOR UPDATE")
        .fetch_one(&mut *tx)
        .await?;
    let is_admin = count == 0;
    let res = sqlx::query("INSERT INTO users (email, name, password_hash, is_admin) VALUES (?, ?, ?, ?)")
        .bind(&email)
        .bind(name)
        .bind(hash)
        .bind(is_admin)
        .execute(&mut *tx)
        .await;
    let id = match res {
        Ok(r) => r.last_insert_id(),
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => return Err(UserError::EmailTaken),
        Err(e) => return Err(e.into()),
    };
    tx.commit().await?;
    Ok(User { id, email, name: name.to_string(), is_admin })
}

pub async fn authenticate(
    pool: &MySqlPool,
    email: &str,
    password: &str,
) -> Result<Option<User>, UserError> {
    let row: Option<(u64, String, String, bool, String)> = sqlx::query_as(
        "SELECT id, email, name, is_admin, password_hash FROM users WHERE email = ?",
    )
    .bind(email.trim().to_lowercase())
    .fetch_optional(pool)
    .await?;
    let Some((id, email, name, is_admin, hash)) = row else {
        return Ok(None);
    };
    let parsed = PasswordHash::new(&hash).map_err(|e| UserError::Hash(e.to_string()))?;
    if Argon2::default().verify_password(password.as_bytes(), &parsed).is_err() {
        return Ok(None);
    }
    Ok(Some(User { id, email, name, is_admin }))
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// Crée une session et renvoie le jeton en clair (seul son hash est stocké).
pub async fn create_session(pool: &MySqlPool, user_id: u64) -> Result<String, UserError> {
    let token = hex::encode(rand::random::<[u8; 32]>());
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, expires_at) VALUES (?, ?, DATE_ADD(NOW(), INTERVAL ? DAY))",
    )
    .bind(hash_token(&token))
    .bind(user_id)
    .bind(SESSION_DAYS)
    .execute(pool)
    .await?;
    Ok(token)
}

pub async fn user_for_session(pool: &MySqlPool, token: &str) -> Result<Option<User>, UserError> {
    Ok(sqlx::query_as(
        "SELECT u.id, u.email, u.name, u.is_admin FROM sessions s \
         JOIN users u ON u.id = s.user_id WHERE s.token_hash = ? AND s.expires_at > NOW()",
    )
    .bind(hash_token(token))
    .fetch_optional(pool)
    .await?)
}

pub async fn delete_session(pool: &MySqlPool, token: &str) -> Result<(), UserError> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
        .bind(hash_token(token))
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn count(pool: &MySqlPool) -> Result<u64, sqlx::Error> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(pool).await?;
    Ok(n as u64)
}

#[derive(Debug, Clone, serde::Serialize, FromRow)]
pub struct UserRow {
    pub id: u64,
    pub email: String,
    pub name: String,
    pub is_admin: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub async fn list(pool: &MySqlPool) -> Result<Vec<UserRow>, sqlx::Error> {
    sqlx::query_as("SELECT id, email, name, is_admin, created_at FROM users ORDER BY id").fetch_all(pool).await
}

pub async fn find_by_email(pool: &MySqlPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as("SELECT id, email, name, is_admin FROM users WHERE email = ?")
        .bind(email.trim().to_lowercase())
        .fetch_optional(pool)
        .await
}

pub async fn set_admin(pool: &MySqlPool, user_id: u64, is_admin: bool) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE users SET is_admin = ? WHERE id = ?").bind(is_admin).bind(user_id).execute(pool).await?;
    Ok(())
}

// --- Authentification à deux facteurs (TOTP) ---

const RECOVERY_CODES: usize = 8;
const CHALLENGE_MINUTES: i64 = 5;
const CHALLENGE_MAX_ATTEMPTS: i64 = 5;

#[derive(Debug, Clone)]
pub struct TwoFactorStatus {
    pub enabled: bool,
    /// Secret généré mais pas encore confirmé par un code.
    pub pending_secret: Option<String>,
    pub recovery_remaining: u64,
}

pub async fn two_factor_enabled(pool: &MySqlPool, user_id: u64) -> Result<bool, sqlx::Error> {
    let enabled: Option<bool> = sqlx::query_scalar("SELECT totp_enabled FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(enabled.unwrap_or(false))
}

pub async fn two_factor_status(pool: &MySqlPool, key: &SecretKey, user_id: u64) -> Result<TwoFactorStatus, sqlx::Error> {
    let (enabled, secret): (bool, Option<String>) =
        sqlx::query_as("SELECT totp_enabled, totp_secret FROM users WHERE id = ?").bind(user_id).fetch_one(pool).await?;
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recovery_codes WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    Ok(TwoFactorStatus {
        enabled,
        pending_secret: if enabled { None } else { secret.and_then(|s| key.decrypt(&s)) },
        recovery_remaining: remaining as u64,
    })
}

/// État sans clé de chiffrement : le secret en attente n'est pas lisible.
pub async fn two_factor_status_without_secret(pool: &MySqlPool, user_id: u64) -> Result<TwoFactorStatus, sqlx::Error> {
    let enabled = two_factor_enabled(pool, user_id).await?;
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM recovery_codes WHERE user_id = ?").bind(user_id).fetch_one(pool).await?;
    Ok(TwoFactorStatus { enabled, pending_secret: None, recovery_remaining: remaining as u64 })
}

/// Génère un nouveau secret en attente de confirmation. `None` si la 2FA est déjà active.
pub async fn begin_two_factor_setup(pool: &MySqlPool, key: &SecretKey, user_id: u64) -> Result<Option<String>, sqlx::Error> {
    let secret = crate::totp::generate_secret();
    let res = sqlx::query("UPDATE users SET totp_secret = ? WHERE id = ? AND totp_enabled = FALSE")
        .bind(key.encrypt(&secret))
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok((res.rows_affected() == 1).then_some(secret))
}

fn normalize_recovery_code(code: &str) -> String {
    code.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase()
}

fn new_recovery_code() -> String {
    let h = hex::encode(rand::random::<[u8; 5]>());
    format!("{}-{}", &h[..5], &h[5..])
}

/// Confirme la configuration avec un code de l'application. Active la 2FA et renvoie les codes de
/// secours (affichés une seule fois : seuls leurs hashs sont stockés) ; `None` si le code est faux.
pub async fn enable_two_factor(
    pool: &MySqlPool,
    key: &SecretKey,
    user_id: u64,
    code: &str,
    now: u64,
) -> Result<Option<Vec<String>>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let row: Option<(Option<String>, bool)> =
        sqlx::query_as("SELECT totp_secret, totp_enabled FROM users WHERE id = ? FOR UPDATE").bind(user_id).fetch_optional(&mut *tx).await?;
    let Some((Some(secret), false)) = row else { return Ok(None) };
    let Some(secret) = key.decrypt(&secret) else { return Ok(None) };
    let Some(step) = crate::totp::verify(&secret, code, now, None) else { return Ok(None) };

    sqlx::query("UPDATE users SET totp_enabled = TRUE, totp_last_step = ? WHERE id = ?")
        .bind(step)
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM recovery_codes WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
    let codes: Vec<String> = (0..RECOVERY_CODES).map(|_| new_recovery_code()).collect();
    for code in &codes {
        sqlx::query("INSERT INTO recovery_codes (user_id, code_hash) VALUES (?, ?)")
            .bind(user_id)
            .bind(hash_token(&normalize_recovery_code(code)))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(Some(codes))
}

pub async fn disable_two_factor(pool: &MySqlPool, user_id: u64) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE users SET totp_enabled = FALSE, totp_secret = NULL, totp_last_step = NULL WHERE id = ?")
        .bind(user_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM recovery_codes WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM login_challenges WHERE user_id = ?").bind(user_id).execute(&mut *tx).await?;
    tx.commit().await
}

/// Vérifie le mot de passe d'un utilisateur déjà connecté (confirmation d'une action sensible).
pub async fn check_password(pool: &MySqlPool, user_id: u64, password: &str) -> Result<bool, UserError> {
    let hash: Option<String> =
        sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?").bind(user_id).fetch_optional(pool).await?;
    let Some(hash) = hash else { return Ok(false) };
    let parsed = PasswordHash::new(&hash).map_err(|e| UserError::Hash(e.to_string()))?;
    Ok(Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
}

/// Second facteur : code de l'application (une seule fois par période) ou code de secours (une fois).
async fn verify_second_factor(pool: &MySqlPool, key: &SecretKey, user_id: u64, code: &str, now: u64) -> Result<bool, sqlx::Error> {
    let row: Option<(Option<String>, Option<u64>)> =
        sqlx::query_as("SELECT totp_secret, totp_last_step FROM users WHERE id = ? AND totp_enabled = TRUE")
            .bind(user_id)
            .fetch_optional(pool)
            .await?;
    let Some((secret, last_step)) = row else { return Ok(false) };
    if let Some(step) = secret.and_then(|s| key.decrypt(&s)).and_then(|s| crate::totp::verify(&s, code, now, last_step)) {
        // Mise à jour conditionnelle : deux requêtes simultanées ne consomment pas le même code.
        let res = sqlx::query("UPDATE users SET totp_last_step = ? WHERE id = ? AND (totp_last_step IS NULL OR totp_last_step < ?)")
            .bind(step)
            .bind(user_id)
            .bind(step)
            .execute(pool)
            .await?;
        return Ok(res.rows_affected() == 1);
    }
    let res = sqlx::query("DELETE FROM recovery_codes WHERE user_id = ? AND code_hash = ?")
        .bind(user_id)
        .bind(hash_token(&normalize_recovery_code(code)))
        .execute(pool)
        .await?;
    Ok(res.rows_affected() == 1)
}

/// Défi de connexion : jeton à usage limité émis une fois le mot de passe validé.
pub async fn create_login_challenge(pool: &MySqlPool, user_id: u64) -> Result<String, sqlx::Error> {
    let token = hex::encode(rand::random::<[u8; 32]>());
    sqlx::query("INSERT INTO login_challenges (token_hash, user_id, expires_at) VALUES (?, ?, DATE_ADD(NOW(), INTERVAL ? MINUTE))")
        .bind(hash_token(&token))
        .bind(user_id)
        .bind(CHALLENGE_MINUTES)
        .execute(pool)
        .await?;
    Ok(token)
}

#[derive(Debug)]
pub enum ChallengeOutcome {
    /// Second facteur correct : le défi est consommé.
    Passed(User),
    /// Code incorrect, le défi reste utilisable (nombre d'essais limité).
    Wrong,
    /// Défi inconnu, expiré ou épuisé : il faut ressaisir le mot de passe.
    Invalid,
}

pub async fn answer_login_challenge(
    pool: &MySqlPool,
    key: &SecretKey,
    token: &str,
    code: &str,
    now: u64,
) -> Result<ChallengeOutcome, sqlx::Error> {
    let hash = hash_token(token);
    // Chaque essai est compté avant d'être vérifié, y compris en cas de requêtes concurrentes.
    let res = sqlx::query(
        "UPDATE login_challenges SET attempts = attempts + 1 WHERE token_hash = ? AND attempts < ? AND expires_at > NOW()",
    )
    .bind(&hash)
    .bind(CHALLENGE_MAX_ATTEMPTS)
    .execute(pool)
    .await?;
    if res.rows_affected() != 1 {
        sqlx::query("DELETE FROM login_challenges WHERE token_hash = ?").bind(&hash).execute(pool).await?;
        return Ok(ChallengeOutcome::Invalid);
    }
    let (user_id,): (u64,) =
        sqlx::query_as("SELECT user_id FROM login_challenges WHERE token_hash = ?").bind(&hash).fetch_one(pool).await?;
    if !verify_second_factor(pool, key, user_id, code, now).await? {
        return Ok(ChallengeOutcome::Wrong);
    }
    // Un défi ne sert qu'une fois ; si un autre appel l'a déjà consommé, on refuse.
    let res = sqlx::query("DELETE FROM login_challenges WHERE token_hash = ?").bind(&hash).execute(pool).await?;
    if res.rows_affected() != 1 {
        return Ok(ChallengeOutcome::Invalid);
    }
    let user = sqlx::query_as("SELECT id, email, name, is_admin FROM users WHERE id = ?").bind(user_id).fetch_one(pool).await?;
    Ok(ChallengeOutcome::Passed(user))
}
