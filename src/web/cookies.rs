//! Cookie de session : lecture de l'en-tête `Cookie` et construction de `Set-Cookie`.

pub const SESSION_COOKIE: &str = "e500_session";

/// Valeur du cookie `name` dans un en-tête `Cookie: a=1; b=2`.
pub fn read(header: &str, name: &str) -> Option<String> {
    header
        .split(';')
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
        .filter(|v| !v.is_empty())
}

fn build(value: &str, max_age: u64, secure: bool) -> String {
    let secure = if secure { "; Secure" } else { "" };
    format!("{SESSION_COOKIE}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax{secure}")
}

pub fn session_cookie(token: &str, max_age_secs: u64, secure: bool) -> String {
    build(token, max_age_secs, secure)
}

pub fn clear_cookie(secure: bool) -> String {
    build("", 0, secure)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_cookie_among_others() {
        assert_eq!(read("a=1; e500_session=tok; b=2", "e500_session").as_deref(), Some("tok"));
        assert_eq!(read("e500_session=only", "e500_session").as_deref(), Some("only"));
        assert_eq!(read("a=1", "e500_session"), None);
        assert_eq!(read("", "e500_session"), None);
        assert_eq!(read("xe500_session=nope", "e500_session"), None);
    }

    #[test]
    fn session_cookie_is_httponly_lax_and_scoped_to_root() {
        let c = session_cookie("tok", 3600, false);
        assert!(c.starts_with("e500_session=tok;"));
        for part in ["HttpOnly", "SameSite=Lax", "Path=/", "Max-Age=3600"] {
            assert!(c.contains(part), "{c} doit contenir {part}");
        }
        assert!(!c.contains("Secure"));
        assert!(session_cookie("tok", 3600, true).contains("Secure"));
    }

    #[test]
    fn clearing_cookie_expires_immediately() {
        let c = clear_cookie(false);
        assert!(c.starts_with("e500_session=;"));
        assert!(c.contains("Max-Age=0"));
    }
}
