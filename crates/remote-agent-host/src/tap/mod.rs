//! HTTP traffic tap: intercept and record the agent CLI's outbound HTTP(S).
//!
//! See [`proxy`] for the MITM/reverse proxy and [`ca`] for the leaf-cert CA.

pub mod ca;
pub mod proxy;
pub mod record;

use std::collections::HashMap;

use shared_protocol::types::{ProviderConfig, TapMode, ToolKind};

/// Env keys used to configure the tap (set by the desktop, stripped before spawn).
pub const ENV_ENABLED: &str = "__tap_enabled";
pub const ENV_MODE: &str = "__tap_mode";

/// Gateway control keys (third-party provider routing).
pub const ENV_GW_PROVIDER: &str = "__gateway_provider";
pub const ENV_GW_TOKEN: &str = "__gateway_token";
pub const ENV_GW_MODE: &str = "__gateway_mode";

/// Provider configs in JSON (model-based routing), set by the frontend.
pub const ENV_PROVIDERS_JSON: &str = "__providers_json";

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
    /// Provider configs for model-based routing (parsed from __providers_json).
    pub providers: Vec<ProviderConfig>,
}

impl TapConfig {
    /// Read (and remove) the tap + gateway control keys from the env map.
    pub fn take_from_env(env: &mut HashMap<String, String>) -> Self {
        let enabled = env
            .remove(ENV_ENABLED)
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let explicit_mode = env.remove(ENV_MODE);
        let gateway_provider = env.remove(ENV_GW_PROVIDER).filter(|v| !v.is_empty());
        let gateway_token = env.remove(ENV_GW_TOKEN).filter(|v| !v.is_empty());
        let gateway_mode = env.remove(ENV_GW_MODE).or_else(|| Some("passthrough".into()));

        // Parse __providers_json for model-based routing.
        let providers_json = env.remove(ENV_PROVIDERS_JSON).filter(|v| !v.is_empty());
        let providers: Vec<ProviderConfig> = providers_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<Vec<ProviderConfig>>(json).ok())
            .unwrap_or_default();
        let has_providers = !providers.is_empty();

        // When a third-party gateway is configured, default to Reverse mode
        // so that ANTHROPIC_BASE_URL is injected and the CLI routes through
        // our local proxy (which prepends the gateway path prefix, e.g. /anthropic).
        // Providers also default to Reverse mode for model-based routing.
        let mode = if gateway_provider.is_some() || gateway_token.is_some() || has_providers {
            match explicit_mode.as_deref() {
                Some("mitm") => TapMode::Mitm,
                _ => TapMode::Reverse,  // default for gateway / providers
            }
        } else {
            match explicit_mode.as_deref() {
                Some("reverse") => TapMode::Reverse,
                _ => TapMode::Mitm,     // default for plain tap (no gateway)
            }
        };
        TapConfig { enabled: enabled || gateway_token.is_some() || has_providers, mode, gateway_provider, gateway_token, gateway_mode, providers }
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

/// Extract host:port from a provider's base URL.
/// E.g., "https://api.deepseek.com/v1" → Some(("api.deepseek.com", 443))
pub fn provider_upstream_host(base_url: &str) -> Option<(String, u16)> {
    let without_scheme = base_url
        .strip_prefix("https://")
        .or_else(|| base_url.strip_prefix("http://"))?;
    let host_part = without_scheme.split('/').next()?;
    let (host, port) = if let Some((h, p)) = host_part.rsplit_once(':') {
        (h.to_string(), p.parse::<u16>().unwrap_or(443))
    } else {
        (
            host_part.to_string(),
            if base_url.starts_with("https://") {
                443
            } else {
                80
            },
        )
    };
    Some((host, port))
}

/// Path prefix for Anthropic-compatible API endpoints on non-Anthropic providers.
/// E.g., DeepSeek's Anthropic endpoint is at /anthropic, not the root.
pub fn provider_path_prefix(provider_kind: &str) -> Option<&'static str> {
    match provider_kind {
        "deepseek" => Some("/anthropic"),
        _ => None,
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
///
/// In **Reverse** mode the CLI base URL is redirected to
/// `http://127.0.0.1:{port}` and a dummy auth value is set so the CLI does
/// NOT block on /login or credential checks.  The proxy injects the real
/// credentials + upstream URL per-request based on model-id matching.
///
/// Only the env vars relevant to the specific `tool` are set — we no longer
/// flood every CLI with unrelated vars.
pub fn proxy_env(
    mode: &TapMode,
    port: u16,
    ca_pem_path: &std::path::Path,
    tool: &ToolKind,
) -> Vec<(String, String)> {
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
                ("REQUESTS_CA_BUNDLE".into(), ca.clone()),
                ("SSL_CERT_FILE".into(), ca),
                ("NO_PROXY".into(), String::new()),
                ("no_proxy".into(), String::new()),
            ]
        }
        TapMode::Reverse => reverse_vars(tool, &endpoint),
    }
}

/// Per-tool env vars for Reverse mode.  Only sets what the specific CLI needs.
fn reverse_vars(tool: &ToolKind, endpoint: &str) -> Vec<(String, String)> {
    match tool {
        // ── Claude Code ──────────────────────────────────────────
        // Only ANTHROPIC_AUTH_TOKEN=dummy is needed; Claude Code
        // uses this (not ANTHROPIC_API_KEY) when BASE_URL points
        // to a non-Anthropic endpoint, skipping the /login prompt.
        // ANTHROPIC_API_KEY is only set in passthrough mode (by the
        // frontend) when the user provides their own API key.
        ToolKind::Claude => vec![
            ("ANTHROPIC_BASE_URL".into(), endpoint.into()),
            (ANTHROPIC_AUTH_TOKEN.into(), "dummy".into()),
        ],

        // ── GitHub Copilot CLI ───────────────────────────────────
        ToolKind::Copilot => vec![
            ("COPILOT_PROVIDER_BASE_URL".into(), endpoint.into()),
            ("COPILOT_PROVIDER_API_KEY".into(), "dummy".into()),
            ("COPILOT_PROVIDER_TYPE".into(), "openai".into()),
            ("COPILOT_OFFLINE".into(), "true".into()),
        ],

        // ── Custom / third-party CLIs ────────────────────────────
        // Most of these speak OpenAI-compatible HTTP (OpenCode, Codex,
        // Hermes with OpenAI backends, etc.), so default to
        // OPENAI_BASE_URL + OPENAI_API_KEY=dummy.  Gemini is an
        // exception: it reads GEMINI_API_KEY and may not support
        // OPENAI_BASE_URL override.
        ToolKind::Custom(name) => {
            let name = name.as_str();
            if name.eq_ignore_ascii_case("gemini") {
                vec![("GEMINI_API_KEY".into(), "dummy".into())]
            } else {
                // opencode / codex / hermes / … — OpenAI-compatible
                vec![
                    ("OPENAI_BASE_URL".into(), endpoint.into()),
                    ("OPENAI_API_BASE".into(), endpoint.into()),
                    ("OPENAI_API_KEY".into(), "dummy".into()),
                ]
            }
        }
    }
}
