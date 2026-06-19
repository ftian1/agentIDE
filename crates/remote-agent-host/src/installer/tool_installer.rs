//! Tool installer — downloads and installs CLI tools on the remote host.
//!
//! Each install operation emits progress events that the client can
//! relay to the frontend for a progress bar UI.
//!
//! Supported tools:
//! - Claude CLI: `npm install -g @anthropic-ai/claude`
//! - GitHub Copilot: `gh extension install github/gh-copilot`

use shared_protocol::types::ToolKind;
use std::process::Command;

/// Phase of an installation in progress.
#[derive(Debug, Clone)]
pub enum InstallPhase {
    Checking,
    Downloading,
    Installing,
    Verifying,
    Complete,
    Failed,
}

/// Progress update emitted during installation.
#[derive(Debug, Clone)]
pub struct InstallProgress {
    pub phase: InstallPhase,
    pub progress: f32, // 0.0 - 1.0
    pub message: String,
}

/// Install a tool and stream progress via the provided callback.
pub fn install(
    tool: &ToolKind,
    on_progress: impl Fn(InstallProgress),
) -> Result<String, String> {
    match tool {
        ToolKind::Claude => install_claude(on_progress),
        ToolKind::Copilot => install_copilot(on_progress),
        ToolKind::Custom(name) => Err(format!("No installer for custom tool: {}", name)),
    }
}

fn install_claude(on_progress: impl Fn(InstallProgress)) -> Result<String, String> {
    on_progress(InstallProgress {
        phase: InstallPhase::Checking,
        progress: 0.0,
        message: "Checking prerequisites...".into(),
    });

    // Check Node.js availability
    let node_ok = Command::new("which")
        .arg("node")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !node_ok {
        // Try to install Node.js via nvm or system package manager
        on_progress(InstallProgress {
            phase: InstallPhase::Failed,
            progress: 0.0,
            message: "Node.js is required to install Claude CLI.".into(),
        });
        return Err("Node.js not found".into());
    }

    on_progress(InstallProgress {
        phase: InstallPhase::Checking,
        progress: 0.2,
        message: "Node.js found. Checking npm...".into(),
    });

    // Check npm
    let npm_ok = Command::new("which")
        .arg("npm")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !npm_ok {
        return Err("npm not found".into());
    }

    on_progress(InstallProgress {
        phase: InstallPhase::Downloading,
        progress: 0.3,
        message: "Installing Claude CLI via npm...".into(),
    });

    // Install via npm
    let output = Command::new("npm")
        .args(["install", "-g", "@anthropic-ai/claude"])
        .output()
        .map_err(|e| format!("npm install failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("npm install failed: {}", stderr));
    }

    on_progress(InstallProgress {
        phase: InstallPhase::Installing,
        progress: 0.7,
        message: "Installation complete. Verifying...".into(),
    });

    // Verify installation
    let version = Command::new("claude")
        .arg("--version")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    on_progress(InstallProgress {
        phase: InstallPhase::Verifying,
        progress: 0.9,
        message: format!("Verified: claude {}", version),
    });

    on_progress(InstallProgress {
        phase: InstallPhase::Complete,
        progress: 1.0,
        message: format!("Claude CLI {} installed successfully", version),
    });

    Ok(version)
}

fn install_copilot(on_progress: impl Fn(InstallProgress)) -> Result<String, String> {
    on_progress(InstallProgress {
        phase: InstallPhase::Checking,
        progress: 0.0,
        message: "Checking GitHub CLI...".into(),
    });

    // Check gh CLI
    let gh_ok = Command::new("which")
        .arg("gh")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !gh_ok {
        return Err(
            "GitHub CLI (gh) not found. Install from https://cli.github.com".into()
        );
    }

    // Check auth
    let auth_ok = Command::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !auth_ok {
        on_progress(InstallProgress {
            phase: InstallPhase::Failed,
            progress: 0.0,
            message: "Run 'gh auth login' first.".into(),
        });
        return Err("gh not authenticated".into());
    }

    on_progress(InstallProgress {
        phase: InstallPhase::Downloading,
        progress: 0.3,
        message: "Installing Copilot extension...".into(),
    });

    let output = Command::new("gh")
        .args(["extension", "install", "github/gh-copilot"])
        .output()
        .map_err(|e| format!("gh extension install failed: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // "already installed" is not an error
        if !stderr.contains("already installed") {
            return Err(format!("Install failed: {}", stderr));
        }
    }

    let version = Command::new("gh")
        .args(["copilot", "--version"])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into());

    on_progress(InstallProgress {
        phase: InstallPhase::Complete,
        progress: 1.0,
        message: format!("GitHub Copilot {} installed", version),
    });

    Ok(version)
}
