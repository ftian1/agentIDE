//! HTTP traffic tap: intercept and record the agent CLI's outbound HTTP(S).
//!
//! See [`proxy`] for the MITM/reverse proxy and [`ca`] for the leaf-cert CA.

pub mod ca;
pub mod proxy;
pub mod record;

use std::collections::HashMap;

use shared_protocol::types::{TapMode, ToolKind};

/// Env keys used to configure the tap (set by the desktop, stripped before spawn).
pub const ENV_ENABLED: &str = "__tap_enabled";
pub const ENV_MODE: &str = "__tap_mode";

/// Gateway control keys (third-party provider routing).
pub const ENV_GW_PROVIDER: &str = "__gateway_provider";
pub const ENV_GW_TOKEN: &str = "__gateway_token";
pub const ENV_GW_MODE: &str = "__gateway_mode";

/// Claude-CLI-only: set to dummy so it doesn't block on /login prompt
/// when ANTHROPIC_BASE_URL points to a non-Anthropic endpoint.
pub const ANTHROPIC_AUTH_TOKEN: &str = "ANTHROPIC_AUTH_TOKEN";

/// Tap configuration parsed from the spawn env map.
pub struct TapConfig {
    pub enabled: bool,
    pub mode: TapMode,
    /// Third-party provider routing (former gateway).
    pub gateway_provider: Option<String>,
    pub gateway_token: Option<String>,
    /// "passthrough" (forward as-is + auth) or "translate" (Anthropic↔OpenAI).
    pub gateway_mode: Option<String>,
}

impl TapConfig {
    /// Read (and remove) the tap + gateway control keys from the env map.
    pub fn take_from_env(env: &mut HashMap<String, String>) -> Self {
        let enabled = env
            .remove(ENV_ENABLED)
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let mode = match env.remove(ENV_MODE).as_deref() {
            Some("reverse") => TapMode::Reverse,
            _ => TapMode::Mitm,
        };
        let gateway_provider = env.remove(ENV_GW_PROVIDER).filter(|v| !v.is_empty());
        let gateway_token = env.remove(ENV_GW_TOKEN).filter(|v| !v.is_empty());
        let gateway_mode = env.remove(ENV_GW_MODE).or_else(|| Some("passthrough".into()));
        TapConfig { enabled, mode, gateway_provider, gateway_token, gateway_mode }
    }
}

/// Check if this provider supports Anthropic format natively (passthrough)
/// vs needing OpenAI translation.
pub fn supports_anthropic_native(provider: &str) -> bool {
    provider.eq_ignore_ascii_case("copilot")
}

/// Default reverse-mode upstream host for a given tool.
pub fn reverse_upstream_for(tool: &ToolKind) -> &'static str {
    match tool {
        ToolKind::Claude => "api.anthropic.com",
        _ => "api.githubcopilot.com",
    }
}

/// An upstream HTTP forward-proxy to use when the agent host cannot reach the
/// internet directly (i.e. corporate proxy environments).
#[derive(Debug, Clone)]
pub struct UpstreamProxy {
    pub host: String,
    pub port: u16,
}

/// Parse a proxy URL of the form `http://host:port` or `host:port`.
/// Returns `None` for empty or unparseable values.
pub fn parse_upstream_proxy(raw: &str) -> Option<UpstreamProxy> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    // Strip http:// or https:// prefix
    let addr = raw
        .strip_prefix("http://")
        .or_else(|| raw.strip_prefix("https://"))
        .unwrap_or(raw);
    let (host, port) = match addr.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(3128)),
        None => (addr.to_string(), 3128),
    };
    if host.is_empty() {
        return None;
    }
    Some(UpstreamProxy { host, port })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_proxy_urls() {
        let p = parse_upstream_proxy("http://proxy-dmz.intel.com:912").unwrap();
        assert_eq!(p.host, "proxy-dmz.intel.com");
        assert_eq!(p.port, 912);

        let p = parse_upstream_proxy("proxy:8080").unwrap();
        assert_eq!(p.host, "proxy");
        assert_eq!(p.port, 8080);

        let p = parse_upstream_proxy("http://proxy.example.com").unwrap();
        assert_eq!(p.host, "proxy.example.com");
        assert_eq!(p.port, 3128); // default

        assert!(parse_upstream_proxy("").is_none());
        assert!(parse_upstream_proxy("  ").is_none());
    }
}

/// Build the env vars to inject so the CLI routes through the tap proxy.
pub fn proxy_env(mode: &TapMode, port: u16, ca_pem_path: &std::path::Path) -> Vec<(String, String)> {
    let endpoint = format!("http://127.0.0.1:{port}");
    match mode {
        TapMode::Mitm => {
            let ca = ca_pem_path.to_string_lossy().to_string();
            vec![
                ("HTTP_PROXY".into(), endpoint.clone()),
                ("http_proxy".into(), endpoint.clone()),
                ("HTTPS_PROXY".into(), endpoint.clone()),
                ("https_proxy".into(), endpoint.clone()),
                ("ALL_PROXY".into(), endpoint.clone()),
                ("all_proxy".into(), endpoint),
                ("NODE_EXTRA_CA_CERTS".into(), ca.clone()),
                // Some tools honor these instead of NODE_EXTRA_CA_CERTS.
                ("REQUESTS_CA_BUNDLE".into(), ca.clone()),
                ("SSL_CERT_FILE".into(), ca),
                // Don't bypass the proxy for localhost-style hosts.
                ("NO_PROXY".into(), String::new()),
                ("no_proxy".into(), String::new()),
            ]
        }
        TapMode::Reverse => {
            // The CLI talks plain HTTP to us; we forward to the real upstream.
            // ANTHROPIC_AUTH_TOKEN=dummy prevents Claude CLI from blocking on /login
            // when the base URL points to a non-Anthropic endpoint.
            vec![
                ("ANTHROPIC_BASE_URL".into(), endpoint.clone()),
                (ANTHROPIC_AUTH_TOKEN.into(), "dummy".into()),
                ("OPENAI_BASE_URL".into(), endpoint.clone()),
                ("OPENAI_API_BASE".into(), endpoint),
            ]
        }
    }
}
