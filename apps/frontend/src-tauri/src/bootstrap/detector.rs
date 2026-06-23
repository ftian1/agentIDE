//! Remote platform detection.
//!
//! Runs basic commands over SSH to determine the remote host's
//! architecture, operating system, and whether the agent is already installed.
//!
//! All detection runs in a SINGLE SSH exec call to minimize round-trips
//! (was 5 sequential commands = ~3s, now 1 = ~0.6s).

use anyhow::Context;

use crate::connection::ssh::{self, SshSession};

/// Information detected about the remote host.
#[derive(Debug, Clone)]
pub struct RemoteInfo {
    pub arch: String,
    pub platform: String,
    pub home_dir: String,
    pub user: String,
    pub agent_version: Option<String>,
    /// SHA256 hash of the installed agent binary (empty if not installed).
    pub agent_sha256: String,
}

/// Run all detection commands in a single SSH exec round-trip.
pub async fn detect(session: &SshSession) -> anyhow::Result<RemoteInfo> {
    let combined = r#"
echo "ARCH=$(uname -m)"
echo "PLATFORM=$(uname -s)"
echo "USER=$(whoami)"
echo "HOME=$HOME"
if [ -x "$HOME/.remote-agent-host/agent" ]; then
    echo "AGENT_VER=$($HOME/.remote-agent-host/agent --version 2>/dev/null)"
    echo "AGENT_SHA256=$(sha256sum $HOME/.remote-agent-host/agent | cut -d' ' -f1)"
else
    echo "AGENT_VER=not_installed"
    echo "AGENT_SHA256="
fi
"#;
    let raw = ssh::exec_remote(session, combined)
        .await
        .context("Failed to detect remote platform")?;

    let mut arch = String::new();
    let mut platform = String::new();
    let mut user = String::new();
    let mut home_dir = String::new();
    let mut agent_version: Option<String> = None;
    let mut agent_sha256 = String::new();

    for line in raw.lines() {
        if let Some(val) = line.strip_prefix("ARCH=") {
            arch = val.to_string();
        } else if let Some(val) = line.strip_prefix("PLATFORM=") {
            platform = val.to_string();
        } else if let Some(val) = line.strip_prefix("USER=") {
            user = val.to_string();
        } else if let Some(val) = line.strip_prefix("HOME=") {
            home_dir = val.to_string();
        } else if let Some(val) = line.strip_prefix("AGENT_VER=") {
            if val != "not_installed" {
                agent_version = Some(val.to_string());
            }
        } else if let Some(val) = line.strip_prefix("AGENT_SHA256=") {
            agent_sha256 = val.trim().to_string();
        }
    }

    anyhow::ensure!(!arch.is_empty(), "Failed to detect architecture");
    anyhow::ensure!(!platform.is_empty(), "Failed to detect platform");
    anyhow::ensure!(!user.is_empty(), "Failed to detect user");
    anyhow::ensure!(!home_dir.is_empty(), "Failed to detect home directory");

    tracing::info!(
        arch = %arch, platform = %platform, user = %user,
        home = %home_dir, agent = ?agent_version, sha256 = %agent_sha256,
        "Remote host detection complete (single round-trip)"
    );

    Ok(RemoteInfo { arch, platform, home_dir, user, agent_version, agent_sha256 })
}
