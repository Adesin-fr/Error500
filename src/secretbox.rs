//! Chiffrement des secrets stockés en base (AES-256-GCM), avec la clé `SECRET_KEY`.
//!
//! Format : `v1:` + base64(nonce de 12 octets ‖ texte chiffré et authentifié).

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

const PREFIX: &str = "v1:";
const NONCE_LEN: usize = 12;

#[derive(Clone)]
pub struct SecretKey(Key<Aes256Gcm>);

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey(…)")
    }
}

impl SecretKey {
    /// Clé de 32 octets en hexadécimal (64 caractères), ex. `openssl rand -hex 32`.
    pub fn from_hex(s: &str) -> Result<Self, String> {
        let bytes = hex::decode(s.trim()).map_err(|_| "doit être en hexadécimal".to_string())?;
        let bytes: [u8; 32] = bytes.try_into().map_err(|_| "doit faire 64 caractères hexadécimaux (32 octets)".to_string())?;
        Ok(SecretKey(bytes.into()))
    }

    pub fn encrypt(&self, plaintext: &str) -> String {
        let nonce: [u8; NONCE_LEN] = rand::random();
        let ct = Aes256Gcm::new(&self.0)
            .encrypt(&Nonce::from(nonce), plaintext.as_bytes())
            .expect("le chiffrement AES-GCM d'un court texte ne peut pas échouer");
        format!("{PREFIX}{}", STANDARD.encode([nonce.as_slice(), &ct].concat()))
    }

    /// `None` si le format est inconnu, la clé mauvaise ou les données altérées.
    pub fn decrypt(&self, sealed: &str) -> Option<String> {
        let raw = STANDARD.decode(sealed.strip_prefix(PREFIX)?).ok()?;
        if raw.len() <= NONCE_LEN {
            return None;
        }
        let (nonce, ct) = raw.split_at(NONCE_LEN);
        let pt = Aes256Gcm::new(&self.0).decrypt(&Nonce::try_from(nonce).ok()?, ct).ok()?;
        String::from_utf8(pt).ok()
    }
}
