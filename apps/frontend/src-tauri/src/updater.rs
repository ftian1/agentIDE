//! Background OTA Updater — periodically checks the GitHub manifest,
//! downloads updated components to the local cache, and notifies the
//! frontend when a restart would apply the updates.
//!
//! Runs entirely on a background thread; the frontend is notified via
//! Tauri events (`update:available`, `update:progress`).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

// ── Config ──────────────────────────────────────────────────────────

const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/ftian1/agentIDE/main/dist/manifest.json";
/// Wait this long after startup before the first check, so the UI has time to load.
const INITIAL_DELAY_SECS: u64 = 10;
/// Interval between subsequent manifest checks.
const CHECK_INTERVAL_SECS: u64 = 3; // 3 seconds
const FETCH_TIMEOUT_SECS: u64 = 15;
const DOWNLOAD_TIMEOUT_SECS: u64 = 120;

// ── Types ───────────────────────────────────────────────────────────

#[derive(Deserialize, Debug, Clone)]
struct Manifest {
    version: String,
    files: HashMap<String, FileEntry>,
}

#[derive(Deserialize, Debug, Clone)]
struct FileEntry {
    sha256: String,
    size: u64,
}

/// Emitted to the frontend when one or more components were updated.
#[derive(Clone, Serialize)]
struct UpdateAvailablePayload {
    version: String,
    updated: Vec<String>,
}

/// Emitted during download for large files.
#[derive(Clone, Serialize)]
struct UpdateProgressPayload {
    file: String,
    downloaded: u64,
    total: u64,
}

// ── Public API ──────────────────────────────────────────────────────

/// Spawn a background task that periodically checks for updates.
/// Call once from `setup`.
pub fn spawn_background_updater(app_handle: AppHandle, cache_dir: PathBuf) {
    tracing::info!(
        "updater: spawning background checker (interval={CHECK_INTERVAL_SECS}s, cache={})",
        cache_dir.display()
    );

    tauri::async_runtime::spawn(async move {
        // Initial delay — let the UI load first.
        tokio::time::sleep(Duration::from_secs(INITIAL_DELAY_SECS)).await;
        tracing::info!("updater: starting first manifest check");

        loop {
            match check_and_update(&app_handle, &cache_dir).await {
                Ok(Some(updated)) => {
                    tracing::info!(
                        "updater: {} component(s) updated, notifying frontend",
                        updated.len()
                    );
                    let _ = app_handle.emit("update:available", UpdateAvailablePayload {
                        version: updated.join(", "),
                        updated: updated.clone(),
                    });
                }
                Ok(None) => {
                    tracing::info!("updater: all components up to date");
                }
                Err(e) => {
                    tracing::warn!("updater: check failed: {e}");
                }
            }

            tracing::info!(
                "updater: sleeping {}s until next check",
                CHECK_INTERVAL_SECS
            );
            tokio::time::sleep(Duration::from_secs(CHECK_INTERVAL_SECS)).await;
        }
    });
}

// ── Core logic ──────────────────────────────────────────────────────

async fn check_and_update(
    app_handle: &AppHandle,
    cache_dir: &Path,
) -> anyhow::Result<Option<Vec<String>>> {
    std::fs::create_dir_all(cache_dir)?;

    // 1. Fetch manifest.
    let manifest = fetch_manifest(cache_dir).await?;
    tracing::info!(
        "updater: manifest version={}, {} files",
        manifest.version,
        manifest.files.len()
    );

    // 2. Compare each file.
    let mut updated = Vec::new();
    for (name, entry) in &manifest.files {
        let local = cache_dir.join(name);
        let needs_update = if local.exists() {
            match sha256_hex(&local) {
                Ok(hash) if hash == entry.sha256 => false,
                Ok(hash) => {
                    tracing::info!(
                        "updater: {} hash mismatch (local {}…, remote {}…)",
                        name,
                        &hash[..16],
                        &entry.sha256[..16]
                    );
                    true
                }
                Err(e) => {
                    tracing::warn!("updater: {} cannot hash local file: {e}", name);
                    true
                }
            }
        } else {
            tracing::info!("updater: {} missing, will download", name);
            true
        };

        if needs_update {
            tracing::info!("updater: downloading {} ({} bytes)", name, entry.size);
            let _ = app_handle.emit("update:progress", UpdateProgressPayload {
                file: name.clone(),
                downloaded: 0,
                total: entry.size,
            });
            download_file(name, entry, cache_dir).await?;
            updated.push(name.clone());
            tracing::info!("updater: {} downloaded and verified", name);
        }
    }

    // 3. Clean stale files.
    let managed: std::collections::HashSet<&str> =
        manifest.files.keys().map(|s| s.as_str()).collect();
    if let Ok(entries) = std::fs::read_dir(cache_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') || name_str == "manifest.json" || name_str.ends_with(".json") {
                continue;
            }
            if !managed.contains(name_str.as_ref()) {
                tracing::info!("updater: removing stale file {}", name_str);
                let path = entry.path();
                if path.is_dir() {
                    std::fs::remove_dir_all(&path).ok();
                } else {
                    std::fs::remove_file(&path).ok();
                }
            }
        }
    }

    if updated.is_empty() {
        Ok(None)
    } else {
        Ok(Some(updated))
    }
}

// ── Manifest fetch ──────────────────────────────────────────────────

// ── Proxy-aware reqwest client ──────────────────────────────────────
// Follows the same env + registry proxy detection as llm.rs, so the
// updater works through corporate / China-network proxies.

fn build_reqwest_client(timeout_secs: u64) -> anyhow::Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(timeout_secs))
        .timeout(Duration::from_secs(timeout_secs));

    if let Some(proxy_url) = detect_proxy_url() {
        match reqwest::Proxy::all(&proxy_url) {
            Ok(p) => {
                tracing::info!(%proxy_url, "updater: using detected proxy");
                builder = builder.proxy(p);
                // Corporate proxies often TLS-intercept — accept their certs.
                builder = builder.danger_accept_invalid_certs(true);
            }
            Err(e) => {
                tracing::warn!(%proxy_url, error = %e, "updater: failed to parse proxy URL, continuing without");
            }
        }
    } else {
        tracing::info!("updater: no proxy detected");
    }

    Ok(builder.build()?)
}

/// Auto-detect the upstream proxy URL, checking:
/// 1. Env vars (HTTPS_PROXY, https_proxy, HTTP_PROXY, http_proxy)
/// 2. Windows IE/Edge proxy settings (registry)
fn detect_proxy_url() -> Option<String> {
    // 1. Env vars
    for key in &["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
        if let Ok(val) = std::env::var(key) {
            let val = val.trim().to_string();
            if !val.is_empty() {
                tracing::info!(key, %val, "updater: proxy from env");
                return Some(val);
            }
        }
    }

    // 2. Windows system proxy (registry)
    #[cfg(target_os = "windows")]
    {
        if let Some(proxy) = detect_windows_proxy() {
            tracing::info!(%proxy, "updater: proxy from winreg");
            return Some(proxy);
        }
    }

    None
}

/// Read the Windows IE/Edge proxy server from the registry.
#[cfg(target_os = "windows")]
fn detect_windows_proxy() -> Option<String> {
    use winreg::enums::*;
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings"
    ) {
        Ok(k) => k,
        Err(_) => return None,
    };
    let enabled: u32 = key.get_value("ProxyEnable").unwrap_or(0);
    if enabled == 0 {
        return None;
    }
    let server: String = match key.get_value("ProxyServer") {
        Ok(s) => s,
        Err(_) => return None,
    };
    let server = server.trim().to_string();
    if server.is_empty() {
        return None;
    }
    // Format can be "host:port" or "http=host:port;https=host:port".
    let url = if let Some(https_part) = server.split(';').find(|s| s.contains("https=")) {
        https_part.trim_start_matches("https=").to_string()
    } else if server.contains('=') {
        server.split(';').next().unwrap_or(&server).split('=').nth(1).unwrap_or(&server).to_string()
    } else {
        server
    };
    let proxy_url = if url.contains("://") { url } else { format!("http://{url}") };
    Some(proxy_url)
}

#[cfg(not(target_os = "windows"))]
fn detect_windows_proxy() -> Option<String> {
    None
}

// ── Manifest fetch ──────────────────────────────────────────────────

async fn fetch_manifest(cache_dir: &Path) -> anyhow::Result<Manifest> {
    let client = build_reqwest_client(FETCH_TIMEOUT_SECS)?;

    tracing::info!("updater: fetching manifest from {MANIFEST_URL}");
    let resp = client
        .get(MANIFEST_URL)
        .header("User-Agent", "remote-ai-ide-updater/0.1")
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("manifest fetch returned {}", resp.status());
    }

    let text = resp.text().await?;

    // Cache the manifest locally for offline comparison.
    let manifest_cache = cache_dir.join("manifest.json");
    std::fs::write(&manifest_cache, &text)?;

    let manifest: Manifest = serde_json::from_str(&text)?;
    Ok(manifest)
}

// ── File download ───────────────────────────────────────────────────

async fn download_file(
    name: &str,
    entry: &FileEntry,
    cache_dir: &Path,
) -> anyhow::Result<()> {
    let base = MANIFEST_URL
        .rsplit_once('/')
        .map(|(b, _)| format!("{b}/"))
        .unwrap_or_default();
    let url = format!("{base}{name}");

    let client = build_reqwest_client(DOWNLOAD_TIMEOUT_SECS)?;

    let resp = client
        .get(&url)
        .header("User-Agent", "remote-ai-ide-updater/0.1")
        .send()
        .await?;

    if !resp.status().is_success() {
        anyhow::bail!("download {url} returned {}", resp.status());
    }

    let tmp = cache_dir.join(format!(".{name}.tmp"));
    let file = std::fs::File::create(&tmp)?;
    let mut hasher = Sha256::new();
    let mut writer = HashingWriter {
        file,
        hasher: &mut hasher,
    };

    let bytes = resp.bytes().await?;
    writer.write_all(&bytes)?;
    writer.flush()?;
    drop(writer);

    let actual = bytes_to_hex(&hasher.finalize());
    if actual != entry.sha256 {
        std::fs::remove_file(&tmp).ok();
        anyhow::bail!(
            "hash mismatch after download: expected {}… got {}…",
            &entry.sha256[..16],
            &actual[..16]
        );
    }

    // If it's a tar.gz, extract it into the frontend/ subdirectory.
    if name.ends_with(".tar.gz") {
        let frontend_dir = cache_dir.join("frontend");
        std::fs::create_dir_all(&frontend_dir)?;
        extract_tar_gz(&tmp, &frontend_dir)?;
        std::fs::remove_file(&tmp).ok();
        tracing::info!("updater: extracted {} -> {}", name, frontend_dir.display());
    } else {
        std::fs::rename(&tmp, cache_dir.join(name))?;
    }

    Ok(())
}

fn extract_tar_gz(tarball: &Path, dest: &Path) -> anyhow::Result<()> {
    let data = std::fs::read(tarball)?;
    let decoder = flate2::read::GzDecoder::new(&data[..]);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(dest)?;
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────

struct HashingWriter<'a> {
    file: std::fs::File,
    hasher: &'a mut Sha256,
}

impl<'a> Write for HashingWriter<'a> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

fn sha256_hex(path: &Path) -> anyhow::Result<String> {
    let data = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&data);
    Ok(bytes_to_hex(&hasher.finalize()))
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{b:02x}");
            s
        })
}
