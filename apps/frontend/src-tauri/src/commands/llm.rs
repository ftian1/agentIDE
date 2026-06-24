//! Tauri IPC commands for multi-provider LLM configuration.
//!
//! Two provider kinds:
//!   - `copilot`           — GitHub Copilot, authenticated via the GitHub OAuth
//!                            device-code flow (mirrors copilot-gateway/auth.py).
//!   - `openai-compatible` — any OpenAI-style endpoint (base URL + API key).
//!
//! Providers and the active-model selection are persisted in the SQLite
//! `settings` KV table, same mechanism as `agent_settings`.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::State;

use crate::AppState;

const PROVIDERS_KEY: &str = "llm_providers";
const ACTIVE_MODEL_KEY: &str = "active_model";

// GitHub OAuth device-code constants, from copilot-gateway/config.py.
const CLIENT_ID: &str = "Ov23li8tweQw6odWQebz";
const API_VERSION: &str = "2026-06-01";
const USER_AGENT: &str = "remote-ai-ide/1.0";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
/// Required by GitHub since 2025 — absent → 404 on /login/device/code.
const COPILOT_INTEGRATION_ID: &str = "vscode-chat";

fn normalize_domain(url: &str) -> String {
    url.replace("https://", "")
        .replace("http://", "")
        .trim_end_matches('/')
        .to_string()
}

fn auth_domain(enterprise_domain: &Option<String>) -> String {
    match enterprise_domain {
        Some(d) if !d.is_empty() => normalize_domain(d),
        _ => "github.com".to_string(),
    }
}

fn copilot_api_base(enterprise_domain: &Option<String>) -> String {
    match enterprise_domain {
        Some(d) if !d.is_empty() => format!("https://copilot-api.{}", normalize_domain(d)),
        _ => "https://api.githubcopilot.com".to_string(),
    }
}

// ── HTTP transport helpers ──────────────────────────────────────
// On Windows we use the system HTTP stack (WinHTTP) so that
// corporate-proxy SSPI authentication (NTLM / Negotiate) is
// handled transparently.  On all other platforms we keep reqwest.

#[cfg(windows)]
mod http {
    use crate::commands::winhttp;
    use serde_json::Value as JsonValue;

    /// POST JSON → JSON.  The caller provides the URL, headers, and a
    /// `serde_json::Value` body.  Returns the parsed response body.
    pub async fn post_json(
        url: &str,
        headers: &[(&str, &str)],
        body: &JsonValue,
    ) -> Result<JsonValue, String> {
        let url = url.to_owned();
        let headers: Vec<(String, String)> =
            headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let body_bytes = serde_json::to_vec(body).map_err(|e| format!("json encode: {e}"))?;

        tokio::task::spawn_blocking(move || {
            let hdrs: Vec<(&str, &str)> =
                headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            let (status, resp) = winhttp::post(&url, &hdrs, &body_bytes)?;
            if status == 0 {
                return Err("http_post: WinHTTP returned status 0".to_string());
            }
            let body_str =
                String::from_utf8_lossy(&resp);
            if !(200..300).contains(&status) {
                let snippet: String = body_str.chars().take(300).collect();
                return Err(format!("HTTP {status} {snippet}"));
            }
            serde_json::from_slice(&resp).map_err(|e| {
                let snippet: String = body_str.chars().take(200).collect();
                format!("json parse error: {e} — body: {snippet}")
            })
        })
        .await
        .map_err(|e| format!("join error: {e}"))?
    }

    /// POST → (status, raw body).  Caller decides how to handle non-2xx.
    pub async fn post_raw(
        url: &str,
        headers: &[(&str, &str)],
        body: &JsonValue,
    ) -> Result<(u16, Vec<u8>), String> {
        let url = url.to_owned();
        let headers: Vec<(String, String)> =
            headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let body_bytes = serde_json::to_vec(body).map_err(|e| format!("json encode: {e}"))?;

        tokio::task::spawn_blocking(move || {
            let hdrs: Vec<(&str, &str)> =
                headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            winhttp::post(&url, &hdrs, &body_bytes)
        })
        .await
        .map_err(|e| format!("join error: {e}"))?
    }

    /// GET → (status, raw body).  Caller parses the body as needed.
    pub async fn get_raw(
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<(u16, Vec<u8>), String> {
        let url = url.to_owned();
        let headers: Vec<(String, String)> =
            headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();

        tokio::task::spawn_blocking(move || {
            let hdrs: Vec<(&str, &str)> =
                headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            winhttp::get(&url, &hdrs)
        })
        .await
        .map_err(|e| format!("join error: {e}"))?
    }
}

#[cfg(not(windows))]
mod http {
    use super::*;
    use serde_json::Value as JsonValue;

    fn client() -> reqwest::Client {
        build_reqwest_client()
    }

    pub async fn post_json(
        url: &str,
        headers: &[(&str, &str)],
        body: &JsonValue,
    ) -> Result<JsonValue, String> {
        let mut req = client().post(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = req
            .json(body)
            .send()
            .await
            .map_err(|e| format!("send failed: {e}"))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let snippet: String = resp
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(300)
                .collect();
            return Err(format!("HTTP {status} {snippet}"));
        }
        resp.json()
            .await
            .map_err(|e| format!("json parse: {e}"))
    }

    pub async fn post_raw(
        url: &str,
        headers: &[(&str, &str)],
        body: &JsonValue,
    ) -> Result<(u16, Vec<u8>), String> {
        let mut req = client().post(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = req
            .json(body)
            .send()
            .await
            .map_err(|e| format!("send failed: {e}"))?;
        let status = resp.status().as_u16();
        let body = resp
            .bytes()
            .await
            .map_err(|e| format!("read body: {e}"))?
            .to_vec();
        Ok((status, body))
    }

    pub async fn get_raw(
        url: &str,
        headers: &[(&str, &str)],
    ) -> Result<(u16, Vec<u8>), String> {
        let mut req = client().get(url);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("send failed: {e}"))?;
        let status = resp.status().as_u16();
        let body = resp
            .bytes()
            .await
            .map_err(|e| format!("read body: {e}"))?
            .to_vec();
        Ok((status, body))
    }
}

// ── device-code auth ────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeResp {
    verification_uri: String,
    user_code: String,
    device_code: String,
    interval: u64,
}

/// Build a reqwest client that respects proxy settings auto-detected from:
/// 1. `HTTPS_PROXY` / `https_proxy` / `HTTP_PROXY` / `http_proxy` env vars
/// 2. Windows system proxy (registry: IE/Edge settings)
///
/// reqwest with rustls-tls does NOT auto-detect system proxies (that requires
/// default-tls / native-tls). This function bridges the gap.
///
/// Only used on non-Windows; Windows uses WinHTTP which handles proxy natively.
#[cfg(not(windows))]
fn build_reqwest_client() -> reqwest::Client {
    let mut builder = reqwest::Client::builder();

    // Log all proxy detection sources for diagnostics.
    let env_https = std::env::var("HTTPS_PROXY").ok();
    let env_http = std::env::var("HTTP_PROXY").ok();
    tracing::info!(
        ?env_https, ?env_http,
        "llm: proxy env read"
    );

    let proxy_url = detect_proxy_url();
    if let Some(ref url) = proxy_url {
        match reqwest::Proxy::all(url) {
            Ok(p) => {
                tracing::info!(%url, "llm: using detected proxy");
                builder = builder.proxy(p);
                // Corporate proxies often TLS-intercept — accept their certs.
                builder = builder.danger_accept_invalid_certs(true);
            }
            Err(e) => {
                tracing::warn!(%url, error = %e, "llm: failed to parse proxy URL, continuing without");
            }
        }
    } else {
        tracing::info!("llm: no proxy detected (env or registry)");
    }

    builder.build().unwrap_or_else(|_| reqwest::Client::new())
}

/// Auto-detect the upstream proxy URL, checking:
/// 1. Env vars (HTTPS_PROXY, https_proxy, HTTP_PROXY, http_proxy)
/// 2. Windows IE/Edge proxy settings (registry)
#[cfg(not(windows))]
fn detect_proxy_url() -> Option<String> {
    // 1. Env vars
    for key in &["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(val) = std::env::var(key) {
            let val = val.trim().to_string();
            if !val.is_empty() {
                return Some(val);
            }
        }
    }

    // 2. Windows system proxy (registry)
    #[cfg(target_os = "windows")]
    {
        if let Some(proxy) = detect_windows_proxy() {
            return Some(proxy);
        }
    }

    None
}

/// Read the Windows IE/Edge proxy server from the registry.
/// Returns `None` if proxy is disabled or not configured.
///
/// Not used on Windows (WinHTTP handles proxy natively).
#[cfg(target_os = "windows")]
#[allow(dead_code)]
fn detect_windows_proxy() -> Option<String> {
    use winreg::enums::*;
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings"
    ) {
        Ok(k) => k,
        Err(e) => {
            tracing::info!(error = %e, "llm: winreg open key failed");
            return None;
        }
    };
    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    tracing::info!(enabled, "llm: winreg ProxyEnable");
    if enabled == 0 {
        return None;
    }
    let server: String = match key.get_value("ProxyServer") {
        Ok(s) => s,
        Err(e) => {
            tracing::info!(error = %e, "llm: winreg ProxyServer read failed");
            return None;
        }
    };
    let server = server.trim().to_string();
    tracing::info!(%server, "llm: winreg ProxyServer raw");
    if server.is_empty() {
        return None;
    }
    // Format can be "host:port" or "http=host:port;https=host:port".
    // Pick the HTTPS proxy if available, otherwise the first one.
    let url = if let Some(https_part) = server.split(';').find(|s| s.contains("https=")) {
        https_part.trim_start_matches("https=").to_string()
    } else if server.contains('=') {
        // format like "http=proxy:80" — take the value after =
        server.split(';').next().unwrap_or(&server).split('=').nth(1).unwrap_or(&server).to_string()
    } else {
        server
    };
    let proxy_url = if url.contains("://") { url } else { format!("http://{url}") };
    tracing::info!(%proxy_url, "llm: winreg resolved proxy");
    Some(proxy_url)
}

// Stub for non-Windows.
#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
fn detect_windows_proxy() -> Option<String> {
    None
}

/// Step 1: initiate the GitHub OAuth device-code flow.
#[tauri::command]
pub async fn copilot_device_start(
    enterprise_domain: Option<String>,
) -> Result<DeviceCodeResp, String> {
    let domain = auth_domain(&enterprise_domain);
    let url = format!("https://{domain}/login/device/code");

    let headers: &[(&str, &str)] = &[
        ("Accept", "application/json"),
        ("Content-Type", "application/json"),
        ("User-Agent", USER_AGENT),
        ("Copilot-Integration-Id", COPILOT_INTEGRATION_ID),
    ];
    let body = serde_json::json!({ "client_id": CLIENT_ID, "scope": "read:user" });

    let data = http::post_json(&url, headers, &body)
        .await
        .map_err(|e| {
            let msg = format!(
                "device code request failed: {e}\n\
                 URL: {url}\n\
                 Hint: if you are on a corporate network, the proxy may be blocking \
                 or TLS-intercepting this connection. Check HTTPS_PROXY env var."
            );
            tracing::error!(%msg);
            msg
        })?;

    Ok(DeviceCodeResp {
        verification_uri: data["verification_uri"].as_str().unwrap_or_default().to_string(),
        user_code: data["user_code"].as_str().unwrap_or_default().to_string(),
        device_code: data["device_code"].as_str().unwrap_or_default().to_string(),
        interval: data["interval"].as_u64().unwrap_or(5),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PollResp {
    /// "pending" | "success" | "failed"
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Step 2: poll once for the access token. The frontend drives the polling
/// loop on the returned `interval` so it can be cancelled when the modal closes.
#[tauri::command]
pub async fn copilot_device_poll(
    device_code: String,
    enterprise_domain: Option<String>,
) -> Result<PollResp, String> {
    let domain = auth_domain(&enterprise_domain);
    let url = format!("https://{domain}/login/oauth/access_token");

    let headers: &[(&str, &str)] = &[
        ("Accept", "application/json"),
        ("Content-Type", "application/json"),
        ("User-Agent", USER_AGENT),
        ("Copilot-Integration-Id", COPILOT_INTEGRATION_ID),
    ];
    let body = serde_json::json!({
        "client_id": CLIENT_ID,
        "device_code": device_code,
        "grant_type": DEVICE_GRANT,
    });

    let (status, resp_body) = http::post_raw(&url, headers, &body)
        .await
        .map_err(|e| format!("token poll failed: {e}"))?;

    if status < 200 || status >= 300 {
        return Ok(PollResp {
            status: "failed".into(),
            access_token: None,
            error: Some(format!("HTTP {}", status)),
        });
    }

    let data: Value =
        serde_json::from_slice(&resp_body).map_err(|e| format!("parse token response: {e}"))?;

    if let Some(token) = data["access_token"].as_str() {
        if !token.is_empty() {
            return Ok(PollResp {
                status: "success".into(),
                access_token: Some(token.to_string()),
                error: None,
            });
        }
    }

    match data["error"].as_str() {
        Some("authorization_pending") | Some("slow_down") => Ok(PollResp {
            status: "pending".into(),
            access_token: None,
            error: None,
        }),
        Some(err) => Ok(PollResp {
            status: "failed".into(),
            access_token: None,
            error: Some(err.to_string()),
        }),
        None => Ok(PollResp {
            status: "pending".into(),
            access_token: None,
            error: None,
        }),
    }
}

// ── model discovery ─────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProviderInput {
    kind: String,
    #[serde(default)]
    base_url: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    copilot_token: Option<String>,
    #[serde(default)]
    enterprise_domain: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

/// Fetch the available models for a provider via its `/models` endpoint.
#[tauri::command]
pub async fn llm_fetch_models(
    provider: LlmProviderInput,
) -> Result<Vec<ModelInfo>, String> {
    // Build url + owned-header-vec. We make the HTTP call inside the same
    // scope so that owned strings live long enough for the header-refs.
    let (status, resp_body) = if provider.kind == "copilot" {
        let base = copilot_api_base(&provider.enterprise_domain);
        let token = provider
            .copilot_token
            .as_deref()
            .filter(|t| !t.is_empty())
            .ok_or("Copilot provider has no token — sign in first")?;
        let auth = format!("Bearer {token}");
        let url = format!("{base}/models");
        let headers: &[(&str, &str)] = &[
            ("Authorization", &auth),
            ("X-GitHub-Api-Version", API_VERSION),
            ("User-Agent", USER_AGENT),
            ("Accept", "application/json"),
        ];
        http::get_raw(&url, headers).await
    } else {
        let base = provider.base_url.trim_end_matches('/');
        if base.is_empty() {
            return Err("base URL is required".into());
        }
        let url = format!("{base}/models");
        let auth: String;
        let headers: Vec<(&str, &str)> =
            if let Some(key) = provider.api_key.as_deref().filter(|k| !k.is_empty()) {
                auth = format!("Bearer {key}");
                vec![("Accept", "application/json"), ("Authorization", &auth)]
            } else {
                vec![("Accept", "application/json")]
            };
        http::get_raw(&url, &headers).await
    }
    .map_err(|e| {
        let base = provider.base_url.trim_end_matches('/');
        format!(
            "model fetch failed: {e}\n\
             Tried: {base}/models\n\
             Hint: verify the base URL is correct and the server is reachable.\n\
             For DeepSeek, use https://api.deepseek.com/v1 (not /anthropic)."
        )
    })?;

    if status < 200 || status >= 300 {
        let snippet: String = String::from_utf8_lossy(&resp_body).chars().take(200).collect();
        return Err(format!("model fetch failed: HTTP {status} {snippet}"));
    }

    let data: Value =
        serde_json::from_slice(&resp_body).map_err(|e| format!("parse models response: {e}"))?;

    let is_copilot = provider.kind == "copilot";
    let list = data["data"].as_array().cloned().unwrap_or_default();
    let mut models = Vec::new();
    for raw in list {
        let id = raw["id"].as_str().unwrap_or_default();
        if id.is_empty() {
            continue;
        }
        if is_copilot {
            // Skip disabled + embedding models, mirroring models.py::_parse_models.
            if raw["policy"]["state"].as_str() == Some("disabled") {
                continue;
            }
            let caps = &raw["capabilities"];
            let family = caps["family"].as_str().unwrap_or(id).to_lowercase();
            let mtype = caps["type"].as_str().unwrap_or_default();
            if family.contains("embedding") || mtype == "embeddings" {
                continue;
            }
        }
        let name = raw["name"]
            .as_str()
            .filter(|n| !n.is_empty())
            .map(|n| n.to_string());
        models.push(ModelInfo {
            id: id.to_string(),
            name,
        });
    }

    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

// ── persistence ─────────────────────────────────────────────────

#[tauri::command]
pub async fn load_llm_providers(state: State<'_, AppState>) -> Result<Option<Value>, String> {
    match state.db.get_setting(PROVIDERS_KEY)? {
        Some(json) => {
            let value: Value =
                serde_json::from_str(&json).map_err(|e| format!("parse providers: {e}"))?;
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn save_llm_providers(
    state: State<'_, AppState>,
    providers: Value,
) -> Result<(), String> {
    let json =
        serde_json::to_string(&providers).map_err(|e| format!("encode providers: {e}"))?;
    state.db.set_setting(PROVIDERS_KEY, &json)?;
    Ok(())
}

#[tauri::command]
pub async fn load_active_model(state: State<'_, AppState>) -> Result<Option<Value>, String> {
    match state.db.get_setting(ACTIVE_MODEL_KEY)? {
        Some(json) => {
            let value: Value =
                serde_json::from_str(&json).map_err(|e| format!("parse active model: {e}"))?;
            Ok(Some(value))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn save_active_model(state: State<'_, AppState>, active: Value) -> Result<(), String> {
    let json = serde_json::to_string(&active).map_err(|e| format!("encode active model: {e}"))?;
    state.db.set_setting(ACTIVE_MODEL_KEY, &json)?;
    Ok(())
}

