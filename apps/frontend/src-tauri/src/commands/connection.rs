//! Tauri IPC commands for SSH connection management.

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::connection::manager::ConnectionInfo;
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    tracing::info!(connection_id = %id, "Connection created");

    Ok(ConnectionInfo {
        id, label, host, port, user,
        status: "disconnected".into(),
        error: None,
        remote_info: None,
    })
}

#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<(), String> {
    state.connections.remove(&connection_id);
    let _ = state.db.delete_connection(&connection_id);
    Ok(())
}

#[tauri::command]
pub async fn list_connections(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionInfo>, String> {
    Ok(state.connections.list())
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
    Ok(vec![])
}
