use error500::totp;

// Vecteurs de la RFC 6238 (SHA-1, secret ASCII "12345678901234567890"), tronqués à 6 chiffres.
const SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

#[test]
fn matches_rfc_6238_vectors() {
    for (t, code) in [(59, "287082"), (1111111109, "081804"), (1111111111, "050471"), (1234567890, "005924"), (2000000000, "279037")] {
        assert_eq!(totp::code_at(SECRET, t).as_deref(), Some(code), "t={t}");
    }
}

#[test]
fn invalid_base32_secret_yields_no_code() {
    assert_eq!(totp::code_at("pas du base32 !", 59), None);
}

#[test]
fn verify_accepts_adjacent_steps_and_returns_the_matched_step() {
    let now = 1111111109;
    let step = now / 30;
    assert_eq!(totp::verify(SECRET, "081804", now, None), Some(step));
    // code de la période précédente (horloge du téléphone légèrement en retard)
    assert_eq!(totp::verify(SECRET, "081804", now + 30, None), Some(step));
    // trop ancien
    assert_eq!(totp::verify(SECRET, "081804", now + 90, None), None);
    assert_eq!(totp::verify(SECRET, "000000", now, None), None);
}

#[test]
fn verify_rejects_a_step_already_used() {
    let now = 1111111109;
    let step = totp::verify(SECRET, "081804", now, None).unwrap();
    assert_eq!(totp::verify(SECRET, "081804", now, Some(step)), None);
}

#[test]
fn verify_ignores_spaces_and_rejects_malformed_input() {
    assert!(totp::verify(SECRET, "081 804", 1111111109, None).is_some());
    assert!(totp::verify(SECRET, "08180", 1111111109, None).is_none());
    assert!(totp::verify(SECRET, "abcdef", 1111111109, None).is_none());
}

#[test]
fn generated_secrets_are_random_160_bit_base32() {
    let a = totp::generate_secret();
    assert_eq!(a.len(), 32);
    assert_ne!(a, totp::generate_secret());
    assert!(totp::code_at(&a, 0).is_some());
}

#[test]
fn otpauth_uri_is_escaped() {
    let uri = totp::otpauth_uri("error500", "a+b@example.com", SECRET);
    assert_eq!(uri, format!("otpauth://totp/error500:a%2Bb%40example.com?secret={SECRET}&issuer=error500"));
}

#[test]
fn qr_code_is_an_svg() {
    assert!(totp::qr_svg("otpauth://totp/x?secret=ABC").starts_with("<?xml"));
}
