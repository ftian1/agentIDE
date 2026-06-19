//! Remote platform detection.
//!
//! Runs basic commands over SSH to determine the remote host's
//! architecture, operating system, and whether the agent is already installed.

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
}

/// Run detection commands on the remote host.
pub async fn detect(session: &SshSession) -> anyhow::Result<RemoteInfo> {
    let arch = ssh::exec_remote(session, "uname -m").await
        .context("Failed to detect architecture")?;
    let platform = ssh::exec_remote(session, "uname -s").await
        .context("Failed to detect platform")?;
    let user = ssh::exec_remote(session, "whoami").await
        .context("Failed to detect user")?;
    let home_dir = ssh::exec_remote(session, "echo $HOME").await
        .context("Failed to detect home directory")?;

    // Check if the agent is already installed
    let agent_version = ssh::exec_remote(
        session,
        "[ -x ~/.remote-agent-host/agent ] && ~/.remote-agent-host/agent --version 2>/dev/null || echo 'not_installed'",
    )
    .await
    .ok()
    .filter(|s| s != "not_installed");

    tracing::info!(
        arch = %arch,
        platform = %platform,
        user = %user,
        home = %home_dir,
        agent = ?agent_version,
        "Remote host detection complete"
    );

    Ok(RemoteInfo {
        arch,
        platform,
        home_dir,
        user,
        agent_version,
    })
}
