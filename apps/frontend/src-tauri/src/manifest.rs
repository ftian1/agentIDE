//! Manifest types shared between startup version-check and OTA updater.
//!
//! `EMBEDDED_MANIFEST_JSON` is the manifest bundled into `loader.exe` at
//! compile time (via build.rs → OUT_DIR).  On startup we compare its version
//! against the cached manifest and use whichever is newer.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Embedded at compile time by build.rs (from dist/manifest.json).
/// Falls back to version "0.0.0.dev" for dev builds without a manifest.
pub const EMBEDDED_MANIFEST_JSON: &str =
    include_str!(concat!(env!("OUT_DIR"), "/embedded_manifest.json"));

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Manifest {
    pub version: String,
    pub files: HashMap<String, FileEntry>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct FileEntry {
    pub sha256: String,
    pub size: u64,
}
