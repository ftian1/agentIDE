//! Per-session HTTP tap proxy.
//!
//! Two modes:
//!  · **MITM**    — the CLI is pointed at us via `HTTPS_PROXY`. We answer the
//!                  `CONNECT host:443` tunnel, TLS-terminate with a leaf cert
//!                  minted by [`TapCa`], then forward each plaintext request to
//!                  the real upstream over TLS, teeing the response.
//!  · **reverse** — the CLI is pointed at us via a base-URL env var (plain HTTP).
//!                  Requests arrive origin-form; we forward to a fixed upstream.
//!
//! Each completed exchange is redacted and sent to the client as
//! `ProtocolMessage::HttpTraffic` over the existing host→client channel.

use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::{Body, Frame, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use shared_protocol::messages::ProtocolMessage;
use shared_protocol::types::ProviderConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::UnboundedSender;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore, ServerConfig};
use tokio_rustls::{TlsAcceptor, TlsConnector};

use super::ca::TapCa;
use super::record::{now_ms, redact_headers, ExchangeBuilder, BODY_CAP};
use super::UpstreamProxy;

/// Handle to a running proxy; aborts the accept loop on drop.
pub struct ProxyHandle {
    pub port: u16,
    pub ca_pem_path: std::path::PathBuf,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for ProxyHandle {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct ProxyState {
    session_id: String,
    transport_tx: UnboundedSender<ProtocolMessage>,
    /// Fallback upstream host (e.g. "api.anthropic.com:443").
    upstream_host: Option<String>,
    /// Corporate forward-proxy for outbound connections (host:port).
    upstream_proxy: Option<UpstreamProxy>,
    /// Third-party provider routing (unified gateway).
    gateway_provider: Option<String>,
    gateway_token: Option<String>,
    /// Path prefix to prepend when forwarding (e.g. "/anthropic" for DeepSeek).
    gateway_path_prefix: Option<String>,
    /// Provider configs for model-based routing (from __providers_json).
    /// When non-empty, takes precedence over old gateway fields.
    providers: Vec<ProviderConfig>,
    ca: TapCa,
    client_config: Arc<ClientConfig>,
    seq: Arc<AtomicU64>,
}

/// Install the ring crypto provider once (rustls needs a process default).
fn ensure_crypto_provider() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        let _ = tokio_rustls::rustls::crypto::ring::default_provider().install_default();
    });
}

#[cfg(test)]
pub(crate) fn ensure_crypto_provider_for_test() {
    ensure_crypto_provider();
}

fn build_client_config() -> Arc<ClientConfig> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let mut cfg = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(cfg)
}

/// Start a Mitm tap proxy bound to an ephemeral 127.0.0.1 port.
pub fn start_session_proxy(
    session_id: String,
    transport_tx: UnboundedSender<ProtocolMessage>,
    upstream_host: Option<String>,
    upstream_proxy: Option<UpstreamProxy>,
    gateway_provider: Option<String>,
    gateway_token: Option<String>,
    gateway_path_prefix: Option<String>,
    providers: Vec<ProviderConfig>,
) -> anyhow::Result<ProxyHandle> {
    ensure_crypto_provider();
    let ca = TapCa::load_or_create()?;
    let ca_pem_path = ca.ca_pem_path().to_path_buf();
    let client_config = build_client_config();

    let std_listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    std_listener.set_nonblocking(true)?;
    let port = std_listener.local_addr()?.port();

    // Log provider configs at startup so we can debug model-id mismatches.
    let provider_count = providers.len();
    for p in &providers {
        tracing::info!(
            provider_id = %p.id,
            provider_kind = %p.kind,
            base_url = %p.base_url,
            model_ids = ?p.model_ids,
            has_api_key = p.api_key.is_some(),
            has_copilot_token = p.copilot_token.is_some(),
            "tap: provider registered"
        );
    }

    let state = Arc::new(ProxyState {
        session_id,
        transport_tx,
        upstream_host,
        upstream_proxy,
        gateway_provider,
        gateway_token,
        gateway_path_prefix,
        providers,
        ca,
        client_config,
        seq: Arc::new(AtomicU64::new(0)),
    });

    let task = tokio::spawn(async move {
        let listener = match TcpListener::from_std(std_listener) {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(error = %e, "tap: listener adopt failed");
                return;
            }
        };
        loop {
            match listener.accept().await {
                Ok((tcp, _)) => {
                    let st = state.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_conn(tcp, st).await {
                            tracing::debug!(error = %e, "tap: connection ended");
                        }
                    });
                }
                Err(e) => {
                    tracing::debug!(error = %e, "tap: accept error");
                    break;
                }
            }
        }
    });

    tracing::info!(port, provider_count, "tap: Mitm proxy started");
    Ok(ProxyHandle { port, ca_pem_path, task })
}

async fn handle_conn(mut tcp: TcpStream, state: Arc<ProxyState>) -> anyhow::Result<()> {
    // MITM: read CONNECT, respond 200, TLS-terminate with leaf cert.
    let target = read_connect_target(&mut tcp).await?;
    let host = target.split(':').next().unwrap_or(&target).to_string();
    tcp.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
    tcp.flush().await?;

    let leaf = state.ca.leaf_for(&host)?;
    let leaf_key = leaf.key();
    let mut server_cfg = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(leaf.chain, leaf_key)
        .map_err(|e| anyhow::anyhow!("server cert: {e}"))?;
    server_cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
    let acceptor = TlsAcceptor::from(Arc::new(server_cfg));
    let tls = acceptor.accept(tcp).await?;
    serve_http(TokioIo::new(tls), host, state).await
}

/// Read bytes until the CONNECT header block terminator, return "host:port".
async fn read_connect_target(tcp: &mut TcpStream) -> anyhow::Result<String> {
    let mut buf = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    loop {
        let n = tcp.read(&mut byte).await?;
        if n == 0 {
            anyhow::bail!("eof before CONNECT terminator");
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
        if buf.len() > 8192 {
            anyhow::bail!("CONNECT header too large");
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let first = text.lines().next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("");
    if !method.eq_ignore_ascii_case("CONNECT") {
        anyhow::bail!("expected CONNECT, got {method}");
    }
    let target = parts.next().unwrap_or("").to_string();
    if target.is_empty() {
        anyhow::bail!("CONNECT without target");
    }
    Ok(target)
}

type IoStream<T> = TokioIo<T>;

/// True when `host` should NOT be routed through a corporate forward-proxy
/// (loopback, link-local, or private RFC 1918 / RFC 6598 / RFC 3927 ranges).
fn is_loopback_or_private(host: &str) -> bool {
    use std::net::IpAddr;
    // Fast path: common names that are never external.
    if host.eq_ignore_ascii_case("localhost")
        || host == "127.0.0.1"
        || host == "::1"
    {
        return true;
    }
    if let Ok(ip) = host.parse::<IpAddr>() {
        return ip.is_loopback()
            || ip.is_unspecified()
            || match ip {
                IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
                IpAddr::V6(v6) => v6.is_unique_local() || v6.is_unicast_link_local(),
            };
    }
    false
}

/// Split `host:port` → `(host, port)`. Falls back to `default_port` if no port
/// is present (or the port segment is unparseable).
fn parse_host_port(host: &str, default_port: u16) -> (String, u16) {
    // IPv6 like [::1]:443
    if let Some(rest) = host.strip_prefix('[') {
        if let Some((ip, port_str)) = rest.split_once("]:") {
            return (ip.to_string(), port_str.parse().unwrap_or(default_port));
        }
    }
    if let Some((h, p)) = host.rsplit_once(':') {
        if let Ok(port) = p.parse::<u16>() {
            return (h.to_string(), port);
        }
    }
    (host.to_string(), default_port)
}

/// Open a TCP connection to `proxy_host:proxy_port`, issue an HTTP CONNECT
/// for `target_host:target_port`, and return the tunneled raw TCP stream on 200.
async fn tunnel_via_proxy(
    proxy_host: &str,
    proxy_port: u16,
    target_host: &str,
    target_port: u16,
) -> anyhow::Result<TcpStream> {
    let mut tcp = TcpStream::connect((proxy_host, proxy_port)).await?;
    let connect_req = format!(
        "CONNECT {target_host}:{target_port} HTTP/1.1\r\n\
         Host: {target_host}:{target_port}\r\n\
         \r\n"
    );
    tcp.write_all(connect_req.as_bytes()).await?;
    tcp.flush().await?;

    // Read the proxy response line + headers until \r\n\r\n.
    let mut buf = Vec::with_capacity(256);
    let mut byte = [0u8; 1];
    loop {
        let n = tcp.read(&mut byte).await?;
        if n == 0 {
            anyhow::bail!("eof from upstream proxy before CONNECT response");
        }
        buf.push(byte[0]);
        if buf.ends_with(b"\r\n\r\n") {
            break;
        }
        if buf.len() > 8192 {
            anyhow::bail!("upstream proxy CONNECT response too large");
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let status_line = text.lines().next().unwrap_or("");
    if !status_line.contains("200") {
        anyhow::bail!("upstream proxy returned: {status_line}");
    }
    Ok(tcp)
}

async fn serve_http<T>(io: IoStream<T>, upstream_host: String, state: Arc<ProxyState>) -> anyhow::Result<()>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let svc = service_fn(move |req: Request<Incoming>| {
        let state = state.clone();
        let upstream_host = upstream_host.clone();
        async move { Ok::<_, hyper::Error>(forward(req, upstream_host, state).await) }
    });

    hyper::server::conn::http1::Builder::new()
        .serve_connection(io, svc)
        .await
        .map_err(|e| anyhow::anyhow!("serve: {e}"))
}

/// Body returned to the client: either the teed upstream body or a small error body.
type OutBody = http_body_util::combinators::BoxBody<Bytes, hyper::Error>;

fn error_response(status: u16, msg: &str) -> Response<OutBody> {
    let body = Full::new(Bytes::from(msg.to_string()))
        .map_err(|never| match never {})
        .boxed();
    Response::builder()
        .status(status)
        .body(body)
        .unwrap_or_else(|_| Response::new(empty_body()))
}

fn empty_body() -> OutBody {
    Full::new(Bytes::new()).map_err(|never| match never {}).boxed()
}

async fn forward(
    req: Request<Incoming>,
    upstream_host: String,
    state: Arc<ProxyState>,
) -> Response<OutBody> {
    let started_at = now_ms();
    let start = std::time::Instant::now();

    let method = req.method().clone();
    let path = req
        .uri()
        .path_and_query()
        .map(|pq| pq.as_str().to_string())
        .unwrap_or_else(|| "/".to_string());

    // Capture request headers (redacted).
    let req_header_pairs: Vec<(String, String)> = req
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let req_headers = redact_headers(req_header_pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())));

    // Buffer the request body (needed to forward it anyway).
    let (parts, body) = req.into_parts();
    let req_body_bytes = match body.collect().await {
        Ok(c) => c.to_bytes(),
        Err(e) => return error_response(502, &format!("read request body: {e}")),
    };

    // ── Determine effective upstream ──────────────────────────────
    // Parse the CLI-intended upstream, then override if a provider or
    // gateway is configured so we connect to the correct API.
    let (cli_upstream_hostname, cli_upstream_port) = parse_host_port(&upstream_host, 443);

    // Parse model_id from request body for model-based routing.
    let model_id = std::str::from_utf8(&req_body_bytes)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .and_then(|v| v.get("model")?.as_str().map(String::from));

    let cli_auth_prefix = parts.headers.get(hyper::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|v| if v.len() > 16 { format!("{}...", &v[..16]) } else { v.to_string() });

    tracing::info!(
        method = %method,
        path = %path,
        target = %cli_upstream_hostname,
        model = ?model_id,
        auth = ?cli_auth_prefix,
        provider_count = state.providers.len(),
        "tap: ← CLI request"
    );

    // Determine routing: providers → model-based, else old gateway, else passthrough.
    let (effective_hostname, effective_port, path_prefix, auth_token) = if !state.providers.is_empty() {
        // ── ROUTING MODE (model-based) ────────────────────────────
        let found = model_id.as_ref().and_then(|model| {
            let provider = state.providers.iter().find(|p| p.model_ids.contains(model))?;
            let kind = &provider.kind;
            let token = provider.api_key.as_deref()
                .or_else(|| provider.copilot_token.as_deref());
            let pp = crate::tap::provider_path_prefix(kind).unwrap_or("");

            let (host, port) = if kind == "copilot" {
                ("api.githubcopilot.com".to_string(), 443u16)
            } else {
                crate::tap::provider_upstream_host(&provider.base_url)
                    .unwrap_or_else(|| (cli_upstream_hostname.clone(), cli_upstream_port))
            };

            tracing::info!(
                model = %model,
                provider_id = %provider.id,
                provider_kind = %kind,
                upstream = %host,
                path_prefix = pp,
                has_auth = token.is_some(),
                "tap: model matched → routing"
            );
            Some((host, port, pp.to_string(), token.map(String::from)))
        });

        match found {
            Some(r) => r,
            None if model_id.is_some() => {
                // Model IS present in request but NOT found in any provider.
                // Fall back to the first provider so the request is routed to
                // a configured third-party endpoint rather than
                // api.anthropic.com (which would 403 on the dummy auth token).
                let model = model_id.as_deref().unwrap_or("?");
                let all_models: Vec<String> = state.providers.iter()
                    .flat_map(|p| p.model_ids.iter().map(|m| format!("{}/{}", p.id, m)))
                    .collect();
                tracing::warn!(
                    requested_model = %model,
                    available_models = ?all_models,
                    "tap: MODEL MISMATCH — requested model not in provider modelIds; falling back to first provider"
                );
                state.providers.first()
                    .and_then(|p| {
                        let kind = &p.kind;
                        let token = p.api_key.as_deref()
                            .or_else(|| p.copilot_token.as_deref());
                        let pp = crate::tap::provider_path_prefix(kind).unwrap_or("");
                        let (host, port) = if kind == "copilot" {
                            ("api.githubcopilot.com".to_string(), 443u16)
                        } else {
                            crate::tap::provider_upstream_host(&p.base_url)
                                .unwrap_or_else(|| (cli_upstream_hostname.clone(), cli_upstream_port))
                        };
                        tracing::info!(
                            provider_id = %p.id,
                            provider_kind = %kind,
                            upstream = %host,
                            has_auth = token.is_some(),
                            "tap: model mismatch → first provider"
                        );
                        Some((host, port, pp.to_string(), token.map(String::from)))
                    })
                    .unwrap_or_else(|| {
                        tracing::error!("tap: model mismatch AND no providers configured — passthrough will fail");
                        (cli_upstream_hostname.clone(), cli_upstream_port, String::new(), None)
                    })
            }
            None => {
                // No model field in request body (preflight / health-check).
                // Use the first provider so the CLI doesn't get a 403 from
                // api.anthropic.com and prompt "please run /login".
                tracing::info!(
                    "tap: no model in request body (preflight), using first provider"
                );
                let fallback = state.providers.first()
                    .and_then(|p| {
                        let kind = &p.kind;
                        let token = p.api_key.as_deref()
                            .or_else(|| p.copilot_token.as_deref());
                        let pp = crate::tap::provider_path_prefix(kind).unwrap_or("");
                        let (host, port) = if kind == "copilot" {
                            ("api.githubcopilot.com".to_string(), 443u16)
                        } else {
                            crate::tap::provider_upstream_host(&p.base_url)
                                .unwrap_or_else(|| (cli_upstream_hostname.clone(), cli_upstream_port))
                        };
                        tracing::info!(
                            provider_id = %p.id,
                            provider_kind = %kind,
                            upstream = %host,
                            has_auth = token.is_some(),
                            "tap: preflight → first provider"
                        );
                        Some((host, port, pp.to_string(), token.map(String::from)))
                    });
                match fallback {
                    Some(r) => r,
                    None => {
                        tracing::warn!("tap: no providers, passthrough");
                        (cli_upstream_hostname.clone(), cli_upstream_port, String::new(), None)
                    }
                }
            }
        }
    } else if let Some(ref gw_provider) = state.gateway_provider {
        // ── OLD GATEWAY ROUTING (backward compat) ─────────────────
        let host = match gw_provider.as_str() {
            "copilot" => "api.githubcopilot.com".to_string(),
            other => format!("api.{other}.com"),
        };
        let pp = state.gateway_path_prefix.as_deref().unwrap_or("").to_string();
        let token = state.gateway_token.clone();
        tracing::info!(provider = %gw_provider, upstream = %host, "tap: old gateway routing");
        (host, cli_upstream_port, pp, token)
    } else if let Some(ref gw_token) = state.gateway_token {
        // Gateway token without provider — just inject auth header.
        tracing::info!("tap: gateway token present (passthrough upstream, inject auth)");
        (cli_upstream_hostname.clone(), cli_upstream_port, String::new(), Some(gw_token.clone()))
    } else {
        // ── PASSTHROUGH MODE ─────────────────────────────────────
        // No providers, no gateway — just forward as-is.
        (cli_upstream_hostname.clone(), cli_upstream_port, String::new(), None)
    };

    // If the CLI already targeted the Anthropic-compatible endpoint (e.g.
    // ANTHROPIC_BASE_URL includes the /anthropic prefix), don't double-apply.
    let effective_path = if !path_prefix.is_empty() && path.starts_with(&path_prefix) {
        path.to_string()
    } else {
        format!("{path_prefix}{path}")
    };
    let effective_url = format!("https://{effective_hostname}{effective_path}");

    // Log the full routing decision for debugging.
    tracing::info!(
        method = %method,
        effective_url = %effective_url,
        upstream = %effective_hostname,
        port = effective_port,
        path_prefix = if path_prefix.is_empty() { "(none)" } else { path_prefix.as_str() },
        has_auth = auth_token.is_some(),
        "tap: forwarding request to upstream"
    );
    // Keep a clone for later logging after the builder consumes the original.
    let log_url = effective_url.clone();

    let builder = ExchangeBuilder {
        exchange_id: uuid::Uuid::new_v4().to_string(),
        method: method.to_string(),
        url: effective_url,
        host: effective_hostname.clone(),
        req_headers,
        req_body: req_body_bytes.to_vec(),
        started_at,
    };

    // Open a connection to the effective upstream — either directly or
    // through a corporate forward-proxy (HTTP CONNECT tunnel). Loopback /
    // private addresses are NOT routed through the corporate proxy.
    let use_proxy = state.upstream_proxy.as_ref()
        .filter(|_| !is_loopback_or_private(&effective_hostname));
    let tcp = if let Some(proxy) = use_proxy {
        match tunnel_via_proxy(&proxy.host, proxy.port, &effective_hostname, effective_port).await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(target = %effective_hostname, %effective_port, error = %e, "tap: connect via proxy FAILED");
                return error_response(502, &format!("upstream proxy tunnel: {e}"));
            }
        }
    } else {
        match TcpStream::connect((effective_hostname.as_str(), effective_port)).await {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(target = %effective_hostname, %effective_port, error = %e, "tap: connect upstream FAILED");
                return error_response(502, &format!("connect upstream: {e}"));
            }
        }
    };
    let server_name = match ServerName::try_from(effective_hostname.clone()) {
        Ok(n) => n,
        Err(e) => {
            tracing::error!(host = %effective_hostname, error = %e, "tap: invalid upstream host");
            return error_response(502, &format!("invalid upstream host: {e}"));
        }
    };
    let connector = TlsConnector::from(state.client_config.clone());
    let tls = match connector.connect(server_name, tcp).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(host = %effective_hostname, error = %e, "tap: upstream TLS FAILED");
            return error_response(502, &format!("upstream TLS: {e}"));
        }
    };

    let (mut sender, conn) = match hyper::client::conn::http1::handshake(TokioIo::new(tls)).await {
        Ok(pair) => pair,
        Err(e) => return error_response(502, &format!("upstream handshake: {e}")),
    };
    tokio::spawn(async move {
        let _ = conn.await;
    });

    // Rebuild the upstream request (origin-form path + original headers/body).
    let mut up_req = Request::builder().method(parts.method).uri(path);
    {
        let headers = up_req.headers_mut().unwrap();
        *headers = parts.headers;
        // Inject provider auth token, replacing the CLI's dummy Bearer.
        if let Some(ref token) = auth_token {
            let prefix = if token.len() > 8 { &token[..8] } else { token };
            tracing::info!(
                token_prefix = %prefix,
                upstream = %effective_hostname,
                "tap: injecting real auth token (replacing CLI dummy)"
            );
            headers.insert(
                hyper::header::AUTHORIZATION,
                format!("Bearer {token}").parse().unwrap(),
            );
        } else {
            tracing::info!(
                upstream = %effective_hostname,
                "tap: no auth token — preserving original Authorization header (passthrough)"
            );
        }
    }
    let up_req = match up_req.body(Full::new(req_body_bytes)) {
        Ok(r) => r,
        Err(e) => return error_response(502, &format!("build upstream request: {e}")),
    };

    // ── Log request before/after proxy modification ─────────────────
    let host_changed = effective_hostname != cli_upstream_hostname;
    let path_changed = !path_prefix.is_empty();
    let auth_changed = auth_token.is_some();
    let modified = host_changed || path_changed || auth_changed;

    if modified {
        tracing::info!(
            method = %method,
            cli_target = %cli_upstream_hostname,
            upstream = %effective_hostname,
            original_path = %log_url,
            path_prefix = if path_prefix.is_empty() { "(none)" } else { &path_prefix },
            auth_injected = auth_changed,
            "tap: REQUEST MODIFIED → forwarding to upstream"
        );
    } else {
        tracing::info!(
            method = %method,
            target = %effective_hostname,
            path = %log_url,
            "tap: REQUEST FORWARDED AS-IS (no modification)"
        );
    }

    let upstream_resp = match sender.send_request(up_req).await {
        Ok(r) => r,
        Err(e) => {
            let elapsed = start.elapsed();
            tracing::warn!(
                method = %method,
                url = %log_url,
                error = %e,
                elapsed_ms = elapsed.as_millis() as u64,
                "tap: upstream request FAILED"
            );
            return error_response(502, &format!("upstream request: {e}"));
        }
    };

    let status = upstream_resp.status();
    let elapsed = start.elapsed();
    tracing::info!(
        method = %method,
        url = %log_url,
        status = status.as_u16(),
        elapsed_ms = elapsed.as_millis() as u64,
        "tap: upstream response received"
    );
    let resp_header_pairs: Vec<(String, String)> = upstream_resp
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let resp_headers = redact_headers(resp_header_pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())));

    let (resp_parts, resp_body) = upstream_resp.into_parts();

    // Tee the streaming response body so the CLI still streams while we capture.
    let tee = TeeBody::new(
        resp_body,
        EmitCtx {
            builder,
            status: status.as_u16(),
            resp_headers,
            duration_start: start,
            transport_tx: state.transport_tx.clone(),
            session_id: state.session_id.clone(),
            seq: state.seq.clone(),
        },
    );

    let mut out = Response::new(tee.boxed());
    *out.status_mut() = status;
    *out.headers_mut() = resp_parts.headers;
    out
}

/// Context needed to emit the completed exchange once the body finishes.
struct EmitCtx {
    builder: ExchangeBuilder,
    status: u16,
    resp_headers: std::collections::HashMap<String, String>,
    duration_start: std::time::Instant,
    transport_tx: UnboundedSender<ProtocolMessage>,
    session_id: String,
    seq: Arc<AtomicU64>,
}

/// Wraps the upstream body: forwards frames downstream while capturing data
/// bytes (capped), then emits the `HttpExchange` at end-of-stream / on drop.
struct TeeBody {
    inner: Incoming,
    captured: Vec<u8>,
    truncated: bool,
    ctx: Option<EmitCtx>,
}

impl TeeBody {
    fn new(inner: Incoming, ctx: EmitCtx) -> Self {
        Self { inner, captured: Vec::new(), truncated: false, ctx: Some(ctx) }
    }

    fn emit(&mut self) {
        let Some(ctx) = self.ctx.take() else { return };
        let duration_ms = ctx.duration_start.elapsed().as_millis() as u64;
        let body = std::mem::take(&mut self.captured);
        let exchange = ctx.builder.finish(
            ctx.status,
            ctx.resp_headers,
            body,
            duration_ms,
            self.truncated,
        );
        let seq = ctx.seq.fetch_add(1, Ordering::SeqCst);
        let _ = ctx.transport_tx.send(ProtocolMessage::HttpTraffic {
            session_id: ctx.session_id,
            exchange,
            seq,
        });
    }
}

impl Body for TeeBody {
    type Data = Bytes;
    type Error = hyper::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(data) = frame.data_ref() {
                    if this.captured.len() < BODY_CAP {
                        let room = BODY_CAP - this.captured.len();
                        if data.len() > room {
                            this.captured.extend_from_slice(&data[..room]);
                            this.truncated = true;
                        } else {
                            this.captured.extend_from_slice(data);
                        }
                    } else if !data.is_empty() {
                        this.truncated = true;
                    }
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(None) => {
                this.emit();
                Poll::Ready(None)
            }
            other => other,
        }
    }
}

impl Drop for TeeBody {
    fn drop(&mut self) {
        // Emit even if the stream was cut short, so partial exchanges surface.
        self.emit();
    }
}
