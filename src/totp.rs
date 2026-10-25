//! Mots de passe à usage unique basés sur le temps (RFC 6238 : SHA-1, 6 chiffres, 30 s),
//! compatibles Google Authenticator.

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, KeyInit, Mac};
use qrcode::render::svg;
use qrcode::QrCode;
use sha1::Sha1;

pub const STEP_SECS: u64 = 30;
const DIGITS: u32 = 6;
/// Périodes acceptées de chaque côté de l'instant courant (dérive d'horloge du téléphone).
const WINDOW: u64 = 1;

/// Secret aléatoire de 160 bits, en base32 (32 caractères).
pub fn generate_secret() -> String {
    BASE32_NOPAD.encode(&rand::random::<[u8; 20]>())
}

fn code_for_step(key: &[u8], step: u64) -> String {
    let mut mac = <Hmac<Sha1> as KeyInit>::new_from_slice(key).expect("HMAC accepte toute longueur de clé");
    mac.update(&step.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let bin = u32::from_be_bytes([digest[offset] & 0x7f, digest[offset + 1], digest[offset + 2], digest[offset + 3]]);
    format!("{:0width$}", bin % 10u32.pow(DIGITS), width = DIGITS as usize)
}

/// Code attendu à l'instant `unix_time` ; `None` si le secret n'est pas du base32 valide.
pub fn code_at(secret: &str, unix_time: u64) -> Option<String> {
    let key = BASE32_NOPAD.decode(secret.as_bytes()).ok()?;
    Some(code_for_step(&key, unix_time / STEP_SECS))
}

/// Vérifie `code` (espaces ignorés) autour de `unix_time`. Renvoie la période correspondante,
/// à mémoriser : un code ne sert qu'une fois, `last_step` rejette toute période déjà consommée.
pub fn verify(secret: &str, code: &str, unix_time: u64, last_step: Option<u64>) -> Option<u64> {
    let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    if code.len() != DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let key = BASE32_NOPAD.decode(secret.as_bytes()).ok()?;
    let now = unix_time / STEP_SECS;
    (now.saturating_sub(WINDOW)..=now + WINDOW)
        .filter(|step| last_step.is_none_or(|last| *step > last))
        .find(|step| constant_time_eq(code_for_step(&key, *step).as_bytes(), code.as_bytes()))
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// URI `otpauth://` lue par les applications d'authentification.
pub fn otpauth_uri(issuer: &str, account: &str, secret: &str) -> String {
    let issuer = percent_encode(issuer);
    format!("otpauth://totp/{issuer}:{}?secret={secret}&issuer={issuer}", percent_encode(account))
}

/// QR code de `data` en SVG (rendu côté serveur : rien n'est envoyé à un service tiers).
pub fn qr_svg(data: &str) -> String {
    let code = QrCode::new(data.as_bytes()).expect("URI otpauth courte, donc encodable");
    code.render::<svg::Color>().min_dimensions(200, 200).build()
}
