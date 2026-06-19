//! Probe service — detects installed CLI tools on the remote host.
//!
//! For each supported tool, runs detection commands to determine:
//! - Whether the tool is installed (`which`)
//! - The installed version (`--version`)
//! - Authentication status (tool-specific)
//! - Additional metadata (plan type, endpoint, etc.)

use shared_protocol::types::ToolKind;
use std::collections::HashMap;
use std::process::Command;

/// Result of probing for a specific CLI tool.
#[derive(Debug, Clone)]
pub struct ProbeOutcome {
    pub tool: ToolKind,
    pub installed: bool,
    pub version: Option<String>,
    pub path: Option<String>,
    pub auth_ok: Option<bool>,
    pub details: HashMap<String, String>,
}

/// Run a command and return stdout + stderr as a string.
fn run_cmd(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Check if a command exists on PATH.
fn which(cmd: &str) -> Option<String> {
    Command::new("which")
        .arg(cmd)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

/// Probe for a CLI tool by name. Returns detailed ProbeOutcome.
pub fn probe(tool: &ToolKind) -> ProbeOutcome {
    match tool {
        ToolKind::Claude => probe_claude(),
        ToolKind::Copilot => probe_copilot(),
        ToolKind::Custom(name) => probe_custom(name),
    }
}

fn probe_claude() -> ProbeOutcome {
    let mut details = HashMap::new();

    let path = which("claude");
    let installed = path.is_some();

    let version = run_cmd("claude", &["--version"]);
    if let Some(ref v) = version {
        details.insert("version_raw".into(), v.clone());
    }

    // Check auth status
    let auth_ok = run_cmd("claude", &["status"])
        .map(|s| s.contains("logged in") || s.contains("authenticated") || s.contains("active"));

    // Try to get plan type and rate limits
    if let Some(status) = run_cmd("claude", &["status", "--json"]) {
        details.insert("status_json".into(), status);
    }

    // Check if it's the npm global install
    if let Some(npm_path) = run_cmd("npm", &["list", "-g", "@anthropic-ai/claude", "--depth=0"]) {
        details.insert("npm_info".into(), npm_path);
    }

    ProbeOutcome {
        tool: ToolKind::Claude,
        installed,
        version,
        path,
        auth_ok,
        details,
    }
}

fn probe_copilot() -> ProbeOutcome {
    let mut details = HashMap::new();

    // Check if gh CLI is installed
    let gh_path = which("gh");
    let gh_version = run_cmd("gh", &["--version"]);

    // Check if copilot extension is installed
    let copilot_installed = if gh_path.is_some() {
        run_cmd("gh", &["extension", "list"])
            .map(|s| s.contains("copilot"))
            .unwrap_or(false)
    } else {
        false
    };

    let version = if copilot_installed {
        run_cmd("gh", &["copilot", "--version"])
    } else {
        None
    };

    // Check auth status
    let auth_ok = run_cmd("gh", &["auth", "status"])
        .map(|s| s.contains("Logged in") || s.contains("Active account"))
        .unwrap_or(false);

    if let Some(ref v) = gh_version {
        details.insert("gh_version".into(), v.clone());
    }

    ProbeOutcome {
        tool: ToolKind::Copilot,
        installed: copilot_installed,
        version,
        path: gh_path,
        auth_ok: Some(auth_ok),
        details,
    }
}

fn probe_custom(name: &str) -> ProbeOutcome {
    let path = which(name);
    let installed = path.is_some();

    let version = if installed {
        run_cmd(name, &["--version"])
            .or_else(|| run_cmd(name, &["-v"]))
            .or_else(|| run_cmd(name, &["version"]))
    } else {
        None
    };

    ProbeOutcome {
        tool: ToolKind::Custom(name.into()),
        installed,
        version,
        path,
        auth_ok: None,
        details: HashMap::new(),
    }
}
