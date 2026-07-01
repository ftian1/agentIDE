//! Background OTA Updater — periodically checks the GitHub manifest,
//! downloads updated components to the local cache, and notifies the
//! frontend when a restart would apply the updates.
//!
//! Runs entirely on a background thread; the frontend is notified via
//! Tauri events (`update:available`, `update:progress`).

use serde::Serialize;
use sha2::{Digest, Sha256};
#[cfg(not(windows))]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::manifest::{FileEntry, Manifest};

// ── Config ──────────────────────────────────────────────────────────

const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/ftian1/agentIDE/main/dist/manifest.json";
/// Wait this long after startup before the first check, so the UI has time to load.
const INITIAL_DELAY_SECS: u64 = 10;
/// Interval between subsequent manifest checks.
const CHECK_INTERVAL_SECS: u64 = 3; // 3 seconds
const FETCH_TIMEOUT_SECS: u64 = 15;
const DOWNLOAD_TIMEOUT_SECS: u64 = 120;

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
                    // quiet — no changes
                }
                Err(e) => {
                    tracing::warn!("updater: check failed: {e}");
                }
            }

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
    tracing::debug!(
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
            // File missing from cache — but the cached manifest (written
            // from embedded on first start) might have a matching hash.
            // If so, the embedded copy is identical to remote → skip download.
            let cached_match = std::fs::read_to_string(cache_dir.join("manifest.json"))
                .ok()
                .and_then(|json| serde_json::from_str::<Manifest>(&json).ok())
                .and_then(|cached| cached.files.get(name).map(|f| f.sha256.clone()))
                .map(|cached_hash| cached_hash == entry.sha256)
                .unwrap_or(false);

            if cached_match {
                tracing::info!(
                    "updater: {} missing but cached manifest hash matches remote — skipping download",
                    name
                );
                false
            } else {
                tracing::info!("updater: {} missing, will download", name);
                true
            }
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
                let path = entry.path();
                // Extracted directories (e.g. frontend/) are managed by
                // their parent tar.gz — don't sweep them.
                if path.is_dir() {
                    continue;
                }
                tracing::info!("updater: removing stale file {}", name_str);
                std::fs::remove_file(&path).ok();
            }
        }
    }

    if updated.is_empty() {
        Ok(None)
    } else {
        Ok(Some(updated))
    }
}

// ── HTTP fetch ──────────────────────────────────────────────────────
// On Windows, the updater uses WinHTTP (same as LLM requests) so proxy
// auto-detection + SSPI auth work transparently, matching Edge / PowerShell.
// On other platforms, reqwest with env-var proxy detection.

#[cfg(windows)]
mod http {
    use super::*;

    /// GET a URL via WinHTTP, returning the response body bytes.
    /// Called from spawn_blocking because WinHTTP is synchronous.
    fn get_blocking(url: &str) -> anyhow::Result<Vec<u8>> {
        let (status, body) =
            crate::commands::winhttp::get(url, &[("User-Agent", "remote-ai-ide-updater/0.1")])
                .map_err(|e| anyhow::anyhow!("WinHTTP: {e}"))?;
        if status < 200 || status >= 300 {
            anyhow::bail!("WinHTTP GET {url} returned {status}");
        }
        Ok(body)
    }

    pub async fn fetch_manifest(cache_dir: &Path) -> anyhow::Result<Manifest> {
        let url = MANIFEST_URL.to_string();
        tracing::debug!("updater: fetching manifest from {url}");

        let text = tokio::task::spawn_blocking(move || {
            let body = get_blocking(&url)?;
            String::from_utf8(body).map_err(|e| anyhow::anyhow!("manifest not UTF-8: {e}"))
        })
        .await??;

        let manifest_cache = cache_dir.join("manifest.json");
        std::fs::write(&manifest_cache, &text)?;

        let manifest: Manifest = serde_json::from_str(&text)?;
        Ok(manifest)
    }

    pub async fn download_file(
        name: &str,
        entry: &FileEntry,
        cache_dir: &Path,
    ) -> anyhow::Result<()> {
        let base = MANIFEST_URL
            .rsplit_once('/')
            .map(|(b, _)| format!("{b}/"))
            .unwrap_or_default();
        let url = format!("{base}{name}");
        let name = name.to_string();
        let cache_dir = cache_dir.to_path_buf();
        let sha256_expected = entry.sha256.clone();

        tracing::info!("updater: downloading {} ({} bytes)", name, entry.size);

        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let bytes = get_blocking(&url)?;

            let actual = bytes_to_hex(&sha2::Sha256::digest(&bytes));
            if actual != sha256_expected {
                anyhow::bail!(
                    "hash mismatch after download: expected {}… got {}…",
                    &sha256_expected[..16],
                    &actual[..16]
                );
            }

            let tmp = cache_dir.join(format!(".{name}.tmp"));
            std::fs::write(&tmp, &bytes)?;

            if name.ends_with(".tar.gz") {
                // Save the tarball first — it's the "managed" file checked
                // for hash comparison on subsequent runs.
                let final_path = cache_dir.join(&name);
                std::fs::rename(&tmp, &final_path)?;
                let frontend_dir = cache_dir.join("frontend");
                std::fs::create_dir_all(&frontend_dir)?;
                super::extract_tar_gz(&final_path, &frontend_dir)?;
                tracing::info!("updater: extracted {} -> {}", name, frontend_dir.display());
            } else {
                std::fs::rename(&tmp, cache_dir.join(&name))?;
            }

            Ok(())
        })
        .await??;
        Ok(())
    }
}

#[cfg(not(windows))]
mod http {
    use super::*;

    fn build_reqwest_client(timeout_secs: u64) -> anyhow::Result<reqwest::Client> {
        let mut builder = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(timeout_secs))
            .timeout(Duration::from_secs(timeout_secs));

        if let Some(proxy_url) = detect_proxy_url() {
            match reqwest::Proxy::all(&proxy_url) {
                Ok(p) => {
                    tracing::info!(%proxy_url, "updater: using detected proxy");
                    builder = builder.proxy(p);
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

    fn detect_proxy_url() -> Option<String> {
        for key in &["HTTPS_PROXY", "https_proxy", "HTTP_PROXY", "http_proxy"] {
            if let Ok(val) = std::env::var(key) {
                let val = val.trim().to_string();
                if !val.is_empty() {
                    tracing::info!(key, %val, "updater: proxy from env");
                    return Some(val);
                }
            }
        }
        None
    }

    pub async fn fetch_manifest(cache_dir: &Path) -> anyhow::Result<Manifest> {
        let client = build_reqwest_client(FETCH_TIMEOUT_SECS)?;

        tracing::debug!("updater: fetching manifest from {MANIFEST_URL}");
        let resp = client
            .get(MANIFEST_URL)
            .header("User-Agent", "remote-ai-ide-updater/0.1")
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("manifest fetch returned {}", resp.status());
        }

        let text = resp.text().await?;

        let manifest_cache = cache_dir.join("manifest.json");
        std::fs::write(&manifest_cache, &text)?;

        let manifest: Manifest = serde_json::from_str(&text)?;
        Ok(manifest)
    }

    pub async fn download_file(
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

        if name.ends_with(".tar.gz") {
            // Save the tarball first for hash comparison on next runs.
            let final_path = cache_dir.join(name);
            std::fs::rename(&tmp, &final_path)?;
            let frontend_dir = cache_dir.join("frontend");
            std::fs::create_dir_all(&frontend_dir)?;
            super::extract_tar_gz(&final_path, &frontend_dir)?;
            tracing::info!("updater: extracted {} -> {}", name, frontend_dir.display());
        } else {
            std::fs::rename(&tmp, cache_dir.join(name))?;
        }

        Ok(())
    }
}

use http::{download_file, fetch_manifest};

fn extract_tar_gz(tarball: &Path, dest: &Path) -> anyhow::Result<()> {
    let data = std::fs::read(tarball)?;
    let decoder = flate2::read::GzDecoder::new(&data[..]);
    let mut archive = tar::Archive::new(decoder);
    archive.unpack(dest)?;
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────

#[cfg(not(windows))]
struct HashingWriter<'a> {
    file: std::fs::File,
    hasher: &'a mut Sha256,
}

#[cfg(not(windows))]
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
