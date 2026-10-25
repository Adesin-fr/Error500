//! Normalisation d'un événement Sentry et calcul de son empreinte de regroupement.

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone)]
pub struct NormalizedEvent {
    pub event_id: String,
    pub level: String,
    pub title: String,
    pub message: Option<String>,
    pub platform: Option<String>,
    pub release: Option<String>,
    pub environment: Option<String>,
    pub transaction: Option<String>,
    pub exception_type: Option<String>,
    pub exception_value: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub fingerprint: String,
}

fn str_field(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Dernière exception de la chaîne (Sentry range la plus récente en dernier).
fn last_exception(payload: &Value) -> Option<&Value> {
    let exc = payload.get("exception")?;
    let values = exc.get("values").unwrap_or(exc);
    values.as_array()?.last()
}

fn parse_timestamp(v: Option<&Value>) -> DateTime<Utc> {
    match v {
        Some(Value::Number(n)) => n
            .as_f64()
            .and_then(|secs| Utc.timestamp_millis_opt((secs * 1000.0) as i64).single()),
        Some(Value::String(s)) => DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc)),
        _ => None,
    }
    .unwrap_or_else(Utc::now)
}

/// Le frame qui identifie le site de l'erreur : dernier frame `in_app`, sinon le dernier.
fn culprit_frame(exception: &Value) -> Option<&Value> {
    let frames = exception.get("stacktrace")?.get("frames")?.as_array()?;
    frames
        .iter()
        .rev()
        .find(|f| f.get("in_app").and_then(Value::as_bool) == Some(true))
        .or_else(|| frames.last())
}

fn default_fingerprint_input(payload: &Value, message: Option<&str>) -> String {
    if let Some(exc) = last_exception(payload) {
        let ty = exc.get("type").and_then(Value::as_str).unwrap_or("");
        return match culprit_frame(exc) {
            Some(frame) => {
                let file = frame
                    .get("module")
                    .or_else(|| frame.get("filename"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let func = frame.get("function").and_then(Value::as_str).unwrap_or("");
                format!("exception\n{ty}\n{file}\n{func}")
            }
            None => {
                let value = exc.get("value").and_then(Value::as_str).unwrap_or("");
                format!("exception\n{ty}\n{value}")
            }
        };
    }
    format!("message\n{}", message.unwrap_or(""))
}

fn fingerprint(payload: &Value, message: Option<&str>) -> String {
    let default = default_fingerprint_input(payload, message);
    let input = match payload.get("fingerprint").and_then(Value::as_array) {
        Some(parts) if !parts.is_empty() => parts
            .iter()
            .filter_map(Value::as_str)
            .map(|p| if p == "{{ default }}" { default.as_str() } else { p })
            .collect::<Vec<_>>()
            .join("\u{1f}"),
        _ => default,
    };
    hex::encode(Sha256::digest(input.as_bytes()))
}

pub fn normalize(payload: &Value) -> NormalizedEvent {
    let exception = last_exception(payload);
    let exception_type = exception.and_then(|e| str_field(e, "type"));
    let exception_value = exception.and_then(|e| str_field(e, "value"));
    let message = str_field(payload, "message").or_else(|| {
        let le = payload.get("logentry")?;
        str_field(le, "formatted").or_else(|| str_field(le, "message"))
    });

    let title = match (&exception_type, &exception_value, &message) {
        (Some(t), Some(v), _) => format!("{t}: {v}"),
        (Some(t), None, _) => t.clone(),
        (None, Some(v), _) => v.clone(),
        (None, None, Some(m)) => m.clone(),
        _ => "<unlabeled event>".to_string(),
    };

    NormalizedEvent {
        event_id: str_field(payload, "event_id")
            .unwrap_or_else(|| hex::encode(rand::random::<[u8; 16]>())),
        level: str_field(payload, "level").unwrap_or_else(|| "error".into()),
        title,
        platform: str_field(payload, "platform"),
        release: str_field(payload, "release"),
        environment: str_field(payload, "environment"),
        transaction: str_field(payload, "transaction"),
        timestamp: parse_timestamp(payload.get("timestamp")),
        fingerprint: fingerprint(payload, message.as_deref()),
        message,
        exception_type,
        exception_value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn exc_event(value: &str, function: &str) -> serde_json::Value {
        json!({
            "event_id": "9ec79c33ec9942ab8353589fcb2e04dc",
            "level": "fatal",
            "platform": "javascript",
            "release": "1.2.3",
            "environment": "prod",
            "transaction": "/checkout",
            "timestamp": 1700000000.5,
            "exception": {"values": [{
                "type": "TypeError",
                "value": value,
                "stacktrace": {"frames": [
                    {"filename": "node_modules/lib.js", "function": "helper", "in_app": false},
                    {"filename": "app.js", "function": function, "in_app": true, "lineno": 10}
                ]}
            }]}
        })
    }

    #[test]
    fn extracts_core_fields() {
        let e = normalize(&exc_event("x is undefined", "pay"));
        assert_eq!(e.event_id, "9ec79c33ec9942ab8353589fcb2e04dc");
        assert_eq!(e.level, "fatal");
        assert_eq!(e.platform.as_deref(), Some("javascript"));
        assert_eq!(e.release.as_deref(), Some("1.2.3"));
        assert_eq!(e.environment.as_deref(), Some("prod"));
        assert_eq!(e.transaction.as_deref(), Some("/checkout"));
        assert_eq!(e.exception_type.as_deref(), Some("TypeError"));
        assert_eq!(e.exception_value.as_deref(), Some("x is undefined"));
        assert_eq!(e.title, "TypeError: x is undefined");
        assert_eq!(e.timestamp.timestamp(), 1_700_000_000);
    }

    #[test]
    fn defaults_when_fields_are_missing() {
        let e = normalize(&json!({}));
        assert_eq!(e.event_id.len(), 32);
        assert_eq!(e.level, "error");
        assert_eq!(e.title, "<unlabeled event>");
        assert!(e.exception_type.is_none());
    }

    #[test]
    fn message_event_title_and_logentry() {
        assert_eq!(normalize(&json!({"message": "hello"})).title, "hello");
        assert_eq!(normalize(&json!({"logentry": {"formatted": "fmt"}})).title, "fmt");
    }

    #[test]
    fn rfc3339_timestamp_is_accepted() {
        let e = normalize(&json!({"timestamp": "2023-11-14T22:13:20Z"}));
        assert_eq!(e.timestamp.timestamp(), 1_700_000_000);
    }

    #[test]
    fn exception_given_as_bare_array_is_accepted() {
        let e = normalize(&json!({"exception": [{"type": "Oops", "value": "v"}]}));
        assert_eq!(e.title, "Oops: v");
    }

    #[test]
    fn same_error_site_with_different_message_shares_fingerprint() {
        let a = normalize(&exc_event("user 1 missing", "pay"));
        let b = normalize(&exc_event("user 2 missing", "pay"));
        assert_eq!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn different_in_app_function_changes_fingerprint() {
        let a = normalize(&exc_event("m", "pay"));
        let b = normalize(&exc_event("m", "refund"));
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    #[test]
    fn explicit_fingerprint_overrides_default() {
        let mut ev = exc_event("m", "pay");
        ev["fingerprint"] = json!(["my-group"]);
        let a = normalize(&ev);
        let mut other = exc_event("different", "other_fn");
        other["fingerprint"] = json!(["my-group"]);
        assert_eq!(a.fingerprint, normalize(&other).fingerprint);
        assert_ne!(a.fingerprint, normalize(&exc_event("m", "pay")).fingerprint);
    }

    #[test]
    fn default_placeholder_extends_default_fingerprint() {
        let plain = normalize(&exc_event("m", "pay"));
        let mut ev = exc_event("m", "pay");
        ev["fingerprint"] = json!(["{{ default }}", "tenant-a"]);
        let a = normalize(&ev);
        assert_ne!(a.fingerprint, plain.fingerprint);
        ev["fingerprint"] = json!(["{{ default }}", "tenant-b"]);
        assert_ne!(a.fingerprint, normalize(&ev).fingerprint);
    }

    #[test]
    fn message_events_group_by_message() {
        let a = normalize(&json!({"message": "disk full"}));
        let b = normalize(&json!({"message": "disk full"}));
        let c = normalize(&json!({"message": "net down"}));
        assert_eq!(a.fingerprint, b.fingerprint);
        assert_ne!(a.fingerprint, c.fingerprint);
    }
}
