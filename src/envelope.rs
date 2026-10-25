//! Parseur du format d'enveloppe Sentry.

use serde::Deserialize;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum EnvelopeError {
    #[error("enveloppe vide")]
    Empty,
    #[error("en-tête d'enveloppe invalide : {0}")]
    Header(String),
    #[error("en-tête d'item invalide : {0}")]
    ItemHeader(String),
    #[error("payload d'item tronqué")]
    Truncated,
}

#[derive(Debug, Deserialize)]
struct EnvelopeHeader {
    event_id: Option<String>,
    dsn: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ItemHeader {
    #[serde(rename = "type")]
    kind: String,
    length: Option<usize>,
}

#[derive(Debug, PartialEq)]
pub struct Item {
    pub kind: String,
    pub payload: Vec<u8>,
}

#[derive(Debug, PartialEq)]
pub struct Envelope {
    pub event_id: Option<String>,
    pub dsn: Option<String>,
    pub items: Vec<Item>,
}

impl Envelope {
    pub fn parse(raw: &[u8]) -> Result<Self, EnvelopeError> {
        let (header_line, mut rest) = split_line(raw);
        if header_line.iter().all(u8::is_ascii_whitespace) {
            return Err(EnvelopeError::Empty);
        }
        let header: EnvelopeHeader = serde_json::from_slice(header_line)
            .map_err(|e| EnvelopeError::Header(e.to_string()))?;

        let mut items = Vec::new();
        while !rest.is_empty() {
            let (line, after) = split_line(rest);
            if line.iter().all(u8::is_ascii_whitespace) {
                rest = after;
                continue;
            }
            let item_header: ItemHeader = serde_json::from_slice(line)
                .map_err(|e| EnvelopeError::ItemHeader(e.to_string()))?;
            let (payload, after) = match item_header.length {
                Some(len) => {
                    if after.len() < len {
                        return Err(EnvelopeError::Truncated);
                    }
                    let (p, tail) = after.split_at(len);
                    (p, tail.strip_prefix(b"\n").unwrap_or(tail))
                }
                None => split_line(after),
            };
            items.push(Item { kind: item_header.kind, payload: payload.to_vec() });
            rest = after;
        }

        Ok(Envelope { event_id: header.event_id, dsn: header.dsn, items })
    }
}

fn split_line(buf: &[u8]) -> (&[u8], &[u8]) {
    match buf.iter().position(|&b| b == b'\n') {
        Some(i) => (&buf[..i], &buf[i + 1..]),
        None => (buf, &[]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_event_envelope() {
        let raw = concat!(
            "{\"event_id\":\"9ec79c33ec9942ab8353589fcb2e04dc\",\"dsn\":\"http://k@localhost/1\"}\n",
            "{\"type\":\"event\",\"length\":18}\n",
            "{\"message\":\"boom\"}\n"
        );
        let env = Envelope::parse(raw.as_bytes()).unwrap();
        assert_eq!(env.event_id.as_deref(), Some("9ec79c33ec9942ab8353589fcb2e04dc"));
        assert_eq!(env.items.len(), 1);
        assert_eq!(env.items[0].kind, "event");
        assert_eq!(env.items[0].payload, b"{\"message\":\"boom\"}");
    }
}
