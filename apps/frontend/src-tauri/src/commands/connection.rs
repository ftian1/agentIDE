//! Tauri IPC commands for SSH connection management.

use serde::{Deserialize, Serialize};
use tauri::{Manager, State};
use uuid::Uuid;

use crate::connection::manager::{ConnectionInfo, RemoteInfo};
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    pub label: Option<String>,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_method: String,
    pub identity_file: Option<String>,
    pub password: Option<String>,
    pub ssh_config_host: Option<String>,
}

#[tauri::command]
pub async fn connect(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    req: ConnectRequest,
) -> Result<ConnectionInfo, String> {
    let host = req.host.clone();
    let port = req.port;
    let user = req.user.clone();

    let id = Uuid::new_v4().to_string();
    let label = req.label.unwrap_or_else(|| format!("{}@{}", user, host));

    let _conn = state.connections.create_connection(
        id.clone(), label.clone(), host.clone(), port, user.clone(),
    );

    let _ = state.db.save_connection(
        &id, &label, &host, port, &user,
        &req.auth_method, req.identity_file.as_deref(),
    );

    tracing::info!(connection_id = %id, "Connection record created");

    // Try SSH connection + bootstrap if the ssh feature is enabled
    #[cfg(feature = "ssh")]
    {
        let ssh_params = crate::connection::ssh::SshConnectionParams {
            host: host.clone(),
            port,
            user: user.clone(),
            auth: match req.auth_method.as_str() {
                "password" => {
                    let pwd = req.password.clone().unwrap_or_default();
                    crate::connection::ssh::AuthMethod::Password(pwd)
                }
                "key" => crate::connection::ssh::AuthMethod::Key(req.identity_file.clone().map(Into::into)),
                "agent" => crate::connection::ssh::AuthMethod::Agent,
                _ => crate::connection::ssh::AuthMethod::Key(None),
            },
        };

        let ssh_session = crate::connection::ssh::connect(&ssh_params).await
            .map_err(|e| format!("SSH connection failed: {}", e))?;

        tracing::info!(connection_id = %id, "SSH connected, starting bootstrap");

        let transport = crate::bootstrap::installer::run_bootstrap(
            &app_handle,
            &id,
            &ssh_session,
        ).await
            .map_err(|e| format!("Bootstrap failed: {}", e))?;

        // Store the transport for session commands.
        // The transport's recv() is single-consumer; exactly ONE demux relay
        // (spawned below) owns it and fans messages out to the frontend keyed
        // by session_id. spawn_session never calls recv() — it awaits an ack
        // oneshot the relay fires — so multiple sessions can't starve each other.
        let transport_arc: std::sync::Arc<dyn crate::transport::Transport> = transport;
        state.connection_transports.insert(id.clone(), transport_arc.clone());

        // Store SSH session for file listing / shell commands
        state.ssh_sessions.insert(id.clone(), std::sync::Arc::new(ssh_session));

        // Spawn the per-connection demux relay (the sole recv() consumer).
        {
            let relay_app = app_handle.clone();
            let relay_id = id.clone();
            let relay_transport = transport_arc.clone();
            tokio::spawn(async move {
                crate::commands::session::connection_demux_relay(relay_transport, relay_app, relay_id).await;
            });
        }

        // Spawn health monitor: periodically check liveness and reconnect if dead
        let monitor_id = id.clone();
        let monitor_app = app_handle.clone();
        let monitor_ssh_params = ssh_params.clone();
        tokio::spawn(async move {
            health_monitor(monitor_id, monitor_ssh_params, monitor_app).await;
        });

        // Spawn perf monitor: periodically sample remote CPU/MEM/Disk and emit
        // `perf:stats` events for the status bar. Exits on disconnect.
        let perf_id = id.clone();
        let perf_app = app_handle.clone();
        tokio::spawn(async move {
            crate::connection::perf::run_perf_monitor(perf_id, perf_app).await;
        });

        tracing::info!(connection_id = %id, "Agent connected and transport stored");

        return Ok(ConnectionInfo {
            id,
            label,
            host,
            port,
            user,
            status: "connected".into(),
            error: None,
            remote_info: Some(RemoteInfo {
                arch: "x86_64".into(),
                agent_version: "0.2.1".into(),
            }),
        });
    }

    // Without ssh feature: just return the saved record as disconnected
    #[cfg(not(feature = "ssh"))]
    {
        tracing::info!(connection_id = %id, "SSH feature disabled — connection saved as disconnected");
        Ok(ConnectionInfo {
            id,
            label,
            host,
            port,
            user,
            status: "disconnected".into(),
            error: None,
            remote_info: None,
        })
    }
}

#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    state.connection_transports.remove(&connection_id);
    state.ssh_sessions.remove(&connection_id);
    state.connections.remove(&connection_id);
    let _ = state.db.delete_connection(&connection_id);
    Ok(())
}

#[tauri::command]
pub async fn list_connections(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionInfo>, String> {
    Ok(state.connections.list().await)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfigHost {
    pub host: String,
    pub hostname: String,
    pub port: u16,
    pub user: String,
    pub identity_file: Option<String>,
}

#[tauri::command]
pub async fn list_ssh_configs() -> Result<Vec<SshConfigHost>, String> {
    let configs = crate::connection::config::parse_ssh_config();
    Ok(configs
        .into_iter()
        .map(|c| SshConfigHost {
            host: c.alias,
            hostname: c.hostname,
            port: c.port,
            user: c.user,
            identity_file: c.identity_file,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Health monitor — runs in background, checks transport liveness, reconnects
// ---------------------------------------------------------------------------

/// Periodically checks whether the SSH transport for a connection is still
/// alive. If the transport dies (e.g. "Connection reset"), attempts to
/// re-establish SSH + bootstrap and update the stored transport.
async fn health_monitor(
    connection_id: String,
    ssh_params: crate::connection::ssh::SshConnectionParams,
    app_handle: tauri::AppHandle,
) {
    // Wait before first check — let the initial connection settle
    tokio::time::sleep(std::time::Duration::from_secs(15)).await;

    loop {
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;

        // Check if transport is still alive
        let transport = {
            let state = app_handle.state::<crate::AppState>();
            state.connection_transports
                .get(&connection_id)
                .map(|r| r.value().clone())
        };
        let alive = match &transport {
            Some(t) => t.is_connected(),
            None => {
                tracing::info!(%connection_id, "Transport removed, health monitor exiting");
                break;
            }
        };

        if alive {
            continue;
        }

        tracing::warn!(%connection_id, "Transport dead, attempting reconnect...");

        // Update connection status
        {
            let state = app_handle.state::<crate::AppState>();
            if let Some(conn) = state.connections.get(&connection_id) {
                let mut status = conn.status.write().await;
                *status = crate::connection::manager::ConnectionStatus::Reconnecting { attempt: 1 };
            }
        }

        // Attempt reconnect
        match try_reconnect(&connection_id, &ssh_params, &app_handle).await {
            Ok(_) => {
                tracing::info!(%connection_id, "Reconnect succeeded");
                {
                    let state = app_handle.state::<crate::AppState>();
                    if let Some(conn) = state.connections.get(&connection_id) {
                        let mut status = conn.status.write().await;
                        *status = crate::connection::manager::ConnectionStatus::Connected;
                    }
                }
            }
            Err(e) => {
                tracing::error!(%connection_id, error = %e, "Reconnect failed, will retry");
                {
                    let state = app_handle.state::<crate::AppState>();
                    if let Some(conn) = state.connections.get(&connection_id) {
                        let mut status = conn.status.write().await;
                        *status = crate::connection::manager::ConnectionStatus::Reconnecting { attempt: 2 };
                    }
                }
            }
        }
    }
}

/// Attempt to re-establish SSH + bootstrap for an existing connection.
/// Updates the stored transport on success.
async fn try_reconnect(
    connection_id: &str,
    ssh_params: &crate::connection::ssh::SshConnectionParams,
    app_handle: &tauri::AppHandle,
) -> anyhow::Result<()> {
    // 1. Re-establish SSH
    let ssh_session = crate::connection::ssh::connect(ssh_params).await?;

    // 2. Bootstrap (upload if needed + start agent)
    let transport = crate::bootstrap::installer::run_bootstrap(
        app_handle,
        connection_id,
        &ssh_session,
    ).await?;

    // 3. Replace stored transport and SSH handle
    let state = app_handle.state::<crate::AppState>();
    let transport_arc: std::sync::Arc<dyn crate::transport::Transport> = transport;
    state.connection_transports.insert(connection_id.to_string(), transport_arc.clone());
    state.ssh_sessions.insert(connection_id.to_string(), std::sync::Arc::new(ssh_session));

    // 4. Spawn a fresh demux relay for the new transport (the old one died with
    //    the old transport).
    {
        let relay_app = app_handle.clone();
        let relay_id = connection_id.to_string();
        tokio::spawn(async move {
            crate::commands::session::connection_demux_relay(transport_arc, relay_app, relay_id).await;
        });
    }

    Ok(())
}
