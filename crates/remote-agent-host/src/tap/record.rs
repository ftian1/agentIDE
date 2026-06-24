//! HTTP exchange recording: assemble captured request/response into an
//! `HttpExchange`, with sensitive auth headers redacted before it leaves the
//! remote host.

use std::collections::HashMap;

use shared_protocol::types::{HttpExchange, TapMode};

/// Max bytes captured per body. Larger bodies are truncated (`truncated: true`).
pub const BODY_CAP: usize = 1024 * 1024; // 1 MiB

const REDACTED: &str = "<redacted>";

/// Header names whose values must never be persisted/streamed.
fn is_sensitive(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "proxy-authorization"
            | "x-api-key"
            | "api-key"
            | "cookie"
            | "set-cookie"
            | "openai-api-key"
            | "anthropic-api-key"
    )
}

/// Convert header pairs to a map, redacting sensitive values.
pub fn redact_headers<'a, I>(headers: I) -> HashMap<String, String>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    headers
        .into_iter()
        .map(|(k, v)| {
            let value = if is_sensitive(k) { REDACTED.to_string() } else { v.to_string() };
            (k.to_string(), value)
        })
        .collect()
}

/// Cap a body to `BODY_CAP`, returning the (possibly truncated) bytes and a flag.
pub fn cap_body(mut body: Vec<u8>) -> (Vec<u8>, bool) {
    if body.len() > BODY_CAP {
        body.truncate(BODY_CAP);
        (body, true)
    } else {
        (body, false)
    }
}

/// Fields collected while proxying, assembled into the wire `HttpExchange`.
pub struct ExchangeBuilder {
    pub exchange_id: String,
    pub method: String,
    pub url: String,
    pub host: String,
    pub req_headers: HashMap<String, String>,
    pub req_body: Vec<u8>,
    pub started_at: u64,
    pub mode: TapMode,
}

impl ExchangeBuilder {
    pub fn finish(
        self,
        status: u16,
        resp_headers: HashMap<String, String>,
        resp_body: Vec<u8>,
        duration_ms: u64,
        truncated: bool,
    ) -> HttpExchange {
        let (req_body, req_trunc) = cap_body(self.req_body);
        let (resp_body, resp_trunc) = cap_body(resp_body);
        HttpExchange {
            exchange_id: self.exchange_id,
            method: self.method,
            url: self.url,
            host: self.host,
            req_headers: self.req_headers,
            req_body,
            status,
            resp_headers,
            resp_body,
            started_at: self.started_at,
            duration_ms,
            mode: self.mode,
            truncated: truncated || req_trunc || resp_trunc,
        }
    }
}

/// Current unix epoch in milliseconds.
pub fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_sensitive_headers_case_insensitively() {
        let headers = vec![
            ("Authorization", "Bearer secret-token"),
            ("X-Api-Key", "sk-12345"),
            ("Content-Type", "application/json"),
            ("Cookie", "session=abc"),
        ];
        let out = redact_headers(headers);
        assert_eq!(out.get("Authorization").unwrap(), "<redacted>");
        assert_eq!(out.get("X-Api-Key").unwrap(), "<redacted>");
        assert_eq!(out.get("Cookie").unwrap(), "<redacted>");
        // Non-sensitive headers pass through untouched.
        assert_eq!(out.get("Content-Type").unwrap(), "application/json");
    }

    #[test]
    fn caps_oversized_bodies() {
        let big = vec![0u8; BODY_CAP + 100];
        let (capped, truncated) = cap_body(big);
        assert_eq!(capped.len(), BODY_CAP);
        assert!(truncated);

        let small = vec![1u8, 2, 3];
        let (kept, truncated) = cap_body(small.clone());
        assert_eq!(kept, small);
        assert!(!truncated);
    }

    #[test]
    fn builder_propagates_truncation_from_either_body() {
        let builder = ExchangeBuilder {
            exchange_id: "x1".into(),
            method: "POST".into(),
            url: "https://api.example.com/v1/messages".into(),
            host: "api.example.com".into(),
            req_headers: std::collections::HashMap::new(),
            req_body: vec![0u8; BODY_CAP + 1], // oversized request body
            started_at: 0,
            mode: TapMode::Mitm,
        };
        let ex = builder.finish(200, std::collections::HashMap::new(), vec![1, 2, 3], 42, false);
        assert_eq!(ex.status, 200);
        assert_eq!(ex.duration_ms, 42);
        assert!(ex.truncated, "request body over cap should mark truncated");
        assert_eq!(ex.req_body.len(), BODY_CAP);
    }
}
