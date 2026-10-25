use error500::secretbox::SecretKey;

const KEY: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

#[test]
fn encrypts_and_decrypts_round_trip() {
    let key = SecretKey::from_hex(KEY).unwrap();
    let sealed = key.encrypt("JBSWY3DPEHPK3PXP");
    assert!(!sealed.contains("JBSWY3DPEHPK3PXP"));
    assert_eq!(key.decrypt(&sealed).as_deref(), Some("JBSWY3DPEHPK3PXP"));
}

#[test]
fn each_encryption_uses_a_fresh_nonce() {
    let key = SecretKey::from_hex(KEY).unwrap();
    assert_ne!(key.encrypt("x"), key.encrypt("x"));
}

#[test]
fn wrong_key_or_tampered_data_does_not_decrypt() {
    let key = SecretKey::from_hex(KEY).unwrap();
    let other = SecretKey::from_hex(&"ab".repeat(32)).unwrap();
    let sealed = key.encrypt("secret");
    assert_eq!(other.decrypt(&sealed), None);
    let mut tampered = sealed.clone();
    tampered.pop();
    tampered.push(if sealed.ends_with('A') { 'B' } else { 'A' });
    assert_eq!(key.decrypt(&tampered), None);
    assert_eq!(key.decrypt("pas chiffré"), None);
}

#[test]
fn key_must_be_64_hex_chars() {
    assert!(SecretKey::from_hex("abcd").is_err());
    assert!(SecretKey::from_hex(&"zz".repeat(32)).is_err());
    assert!(SecretKey::from_hex(&format!(" {KEY}\n")).is_ok());
}

#[test]
fn debug_output_never_shows_the_key() {
    let key = SecretKey::from_hex(KEY).unwrap();
    assert!(!format!("{key:?}").contains("0102"));
}
