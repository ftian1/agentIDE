//! OTA Loader — fetches manifest from GitHub, verifies the local cache,
//! downloads updated components, and launches the main application.
//!
//! The loader itself is intentionally small and rarely changes.  All
//! actual application logic lives in the components it downloads.

use anyhow::Context;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
mod winhttp;

// ── HTTP client (platform-specific) ────────────────────────────────

/// GET a URL, returning the response body bytes.
/// On Windows this uses WinHTTP (system proxy auto-detection).
/// On other platforms it uses reqwest (blocking).
fn http_get(url: &str) -> anyhow::Result<Vec<u8>> {
    #[cfg(windows)]
    {
        winhttp::get(url).map_err(|e| anyhow::anyhow!("WinHTTP: {e}"))
    }
    #[cfg(not(windows))]
    {
        let client = reqwest::blocking::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .build()
            .context("build HTTP client")?;
        let resp = client
            .get(url)
            .header("User-Agent", "ota-loader/0.1")
            .send()
            .context("HTTP GET")?;
        if !resp.status().is_success() {
            anyhow::bail!("HTTP GET {url} returned {}", resp.status());
        }
        Ok(resp.bytes().context("read body")?.to_vec())
    }
}

// ── Config ──────────────────────────────────────────────────────────

const MANIFEST_URL: &str =
    "https://raw.githubusercontent.com/ftian1/agentIDE/main/dist/manifest.json";
const MANIFEST_TTL_SECS: u64 = 300; // 5 min — don't hammer GitHub on restart
const CONNECT_TIMEOUT_SECS: u64 = 15;

// ── Types ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Manifest {
    version: String,
    files: HashMap<String, FileEntry>,
}

#[derive(Deserialize)]
struct FileEntry {
    sha256: String,
    size: u64,
}

// ── Entry point ─────────────────────────────────────────────────────

fn main() {
    if let Err(e) = run() {
        eprintln!("[loader] FATAL: {e}");
        // Keep the window open long enough to read the error.
        std::thread::sleep(std::time::Duration::from_secs(5));
        std::process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let cache = cache_dir();
    std::fs::create_dir_all(&cache).context("create cache dir")?;
    log!("Cache dir: {}", cache.display());

    // 1. Fetch manifest (with short-lived local cache to avoid rate-limiting).
    let manifest = fetch_manifest(&cache)?;
    log!("Manifest version: {}", manifest.version);

    // 2. Verify / download each file.
    for (name, entry) in &manifest.files {
        sync_file(&cache, name, entry)?;
    }

    // 3. Clean up stale files not in the manifest.
    clean_stale(&cache, &manifest.files);

    // 4. Launch the main application.
    launch_main(&cache)?;

    Ok(())
}

// ── Cache directory ─────────────────────────────────────────────────

fn cache_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        // %LOCALAPPDATA%/remote-ai-ide/cache/
        if let Ok(dir) = std::env::var("LOCALAPPDATA") {
            return PathBuf::from(dir).join("remote-ai-ide").join("cache");
        }
    }
    #[cfg(target_os = "linux")]
    {
        // $XDG_DATA_HOME/remote-ai-ide/cache/ or ~/.local/share/remote-ai-ide/cache/
        if let Ok(dir) = std::env::var("XDG_DATA_HOME") {
            return PathBuf::from(dir).join("remote-ai-ide").join("cache");
        }
        if let Ok(dir) = std::env::var("HOME") {
            return PathBuf::from(dir).join(".local").join("share").join("remote-ai-ide").join("cache");
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Ok(dir) = std::env::var("HOME") {
            return PathBuf::from(dir).join("Library").join("Application Support").join("remote-ai-ide").join("cache");
        }
    }
    PathBuf::from("./cache")
}

// ── Manifest fetch ──────────────────────────────────────────────────

fn fetch_manifest(cache: &Path) -> anyhow::Result<Manifest> {
    let manifest_cache = cache.join("manifest.json");

    // Use cached manifest if it's fresh enough.
    if let Ok(meta) = std::fs::metadata(&manifest_cache) {
        if let Ok(mtime) = meta.modified() {
            if let Ok(age) = mtime.elapsed() {
                if age.as_secs() < MANIFEST_TTL_SECS {
                    log!("Using cached manifest ({}s old)", age.as_secs());
                    let data = std::fs::read_to_string(&manifest_cache)?;
                    return serde_json::from_str(&data).context("parse cached manifest");
                }
            }
        }
    }

    log!("Fetching manifest from {}", MANIFEST_URL);
    let text = match http_get(MANIFEST_URL) {
        Ok(body) => String::from_utf8(body).context("manifest not UTF-8")?,
        Err(e) => {
            // If we can't reach GitHub, try the cached manifest even if stale.
            if manifest_cache.exists() {
                log!("GitHub unreachable ({}), using stale cached manifest", e);
                let data = std::fs::read_to_string(&manifest_cache)?;
                return serde_json::from_str(&data).context("parse stale manifest");
            }
            anyhow::bail!("manifest fetch failed: {e} — no cached fallback");
        }
    };
    // Save to disk for TTL-based reuse.
    std::fs::write(&manifest_cache, &text).context("write manifest cache")?;

    let manifest: Manifest = serde_json::from_str(&text).context("parse manifest")?;
    Ok(manifest)
}

// ── File sync ───────────────────────────────────────────────────────

fn sync_file(cache: &Path, name: &str, entry: &FileEntry) -> anyhow::Result<()> {
    let local = cache.join(name);

    // Check if the local file matches.
    if local.exists() {
        match sha256_hex(&local) {
            Ok(hash) if hash == entry.sha256 => {
                log!("  ✓ {} (hash match)", name);
                return Ok(());
            }
            Ok(hash) => {
                log!("  ✗ {} hash mismatch (local={}… remote={}…)", name, &hash[..16], &entry.sha256[..16]);
            }
            Err(_) => {
                log!("  ✗ {} cannot read local file", name);
            }
        }
    } else {
        log!("  ↓ {} (missing, downloading {} bytes)", name, entry.size);
    }

    // Download.
    let url = manifest_base_url() + name;
    let data = http_get(&url).with_context(|| format!("download {url}"))?;

    // Verify hash.
    let mut hasher = Sha256::new();
    hasher.update(&data);
    let actual_hash = bytes_to_hex(&hasher.finalize());
    if actual_hash != entry.sha256 {
        anyhow::bail!(
            "hash mismatch after download: expected {}… got {}…",
            &entry.sha256[..16],
            &actual_hash[..16]
        );
    }

    // Atomic write.
    let tmp = cache.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, &data).context("write tmp file")?;
    std::fs::rename(&tmp, &local).context("atomic rename")?;
    log!("  ✓ {} downloaded and verified ({} bytes)", name, data.len());
    Ok(())
}

// ── Stale cleanup ───────────────────────────────────────────────────

fn clean_stale(cache: &Path, manifest_files: &HashMap<String, FileEntry>) {
    // Files we manage: everything except manifest.json cache and tmp files.
    let managed: std::collections::HashSet<&str> =
        manifest_files.keys().map(|s| s.as_str()).collect();

    if let Ok(entries) = std::fs::read_dir(cache) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') || name_str == "manifest.json" {
                continue;
            }
            if !managed.contains(name_str.as_ref()) {
                let path = entry.path();
                log!("  ✕ removing stale file: {}", path.display());
                if path.is_dir() {
                    std::fs::remove_dir_all(&path).ok();
                } else {
                    std::fs::remove_file(&path).ok();
                }
            }
        }
    }
}

// ── Launch ───────────────────────────────────────────────────────────

fn launch_main(cache: &Path) -> anyhow::Result<()> {
    // The main executable should be in the cache as "main.exe" (Windows)
    // or alongside the loader.  Try cache first, then fallback to the
    // directory containing the loader itself.

    let exe_name = if cfg!(target_os = "windows") {
        "main.exe"
    } else {
        "main"
    };

    let exe_path = cache.join(exe_name);

    // If main.exe isn't in the cache yet (first ever run), try the
    // loader's own directory as a fallback.
    let exe_path = if exe_path.exists() {
        exe_path
    } else if let Ok(loader_dir) = std::env::current_exe().and_then(|p| {
        p.parent()
            .map(|d| d.to_path_buf())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no parent"))
    }) {
        let fallback = loader_dir.join(exe_name);
        if fallback.exists() {
            log!("Using fallback main.exe from loader directory");
            fallback
        } else {
            anyhow::bail!(
                "main.exe not found in cache ({}) or loader directory ({})",
                exe_path.display(),
                loader_dir.display()
            );
        }
    } else {
        exe_path
    };

    log!("Launching: {}", exe_path.display());
    let status = Command::new(&exe_path)
        .current_dir(cache)
        .spawn()
        .with_context(|| format!("launch {}", exe_path.display()))?
        .wait()
        .context("wait for main process")?;

    if !status.success() {
        log!("Main process exited with: {status}");
    }
    Ok(())
}

// ── Helpers ────────────────────────────────────────────────────────

fn manifest_base_url() -> String {
    // Strip "manifest.json" from the URL to get the base directory.
    MANIFEST_URL
        .rsplit_once('/')
        .map(|(base, _)| format!("{base}/"))
        .unwrap_or_else(|| MANIFEST_URL.to_string())
}

fn sha256_hex(path: &Path) -> anyhow::Result<String> {
    let data = std::fs::read(path).context("read file for hash")?;
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

macro_rules! log {
    ($($arg:tt)*) => {
        let msg = format!($($arg)*);
        eprintln!("[loader] {msg}");
    };
}
use log;
