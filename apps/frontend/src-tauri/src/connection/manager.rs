//! Connection Manager — tracks all connections, their lifecycle, and reconnection.

use dashmap::DashMap;
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::RwLock;

pub type ConnectionId = String;

pub struct ConnectionManager {
    connections: DashMap<ConnectionId, Arc<Connection>>,
    #[allow(dead_code)]
    app_handle: AppHandle,
    /// Reconnection configuration.
    reconnect_config: ReconnectConfig,
}

#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    pub max_attempts: u32,
    pub base_delay_ms: u64,
    pub max_delay_ms: u64,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
        }
    }
}

pub struct Connection {
    pub id: ConnectionId,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub status: RwLock<ConnectionStatus>,
    pub remote_info: RwLock<Option<RemoteHostInfo>>,
    pub reconnect_attempt: RwLock<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    Disconnected,
    Connecting,
    Bootstrapping,
    Connected,
    Reconnecting { attempt: u32 },
    Error(String),
}

#[derive(Debug, Clone)]
pub struct RemoteHostInfo {
    pub arch: String,
    pub platform: String,
    pub home_dir: String,
    pub user: String,
    pub agent_version: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub status: String,
    pub error: Option<String>,
    pub remote_info: Option<RemoteInfo>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteInfo {
    pub arch: String,
    pub agent_version: String,
}

fn status_to_string(s: &ConnectionStatus) -> String {
    match s {
        ConnectionStatus::Disconnected => "disconnected".into(),
        ConnectionStatus::Connecting => "connecting".into(),
        ConnectionStatus::Bootstrapping => "bootstrapping".into(),
        ConnectionStatus::Connected => "connected".into(),
        ConnectionStatus::Reconnecting { attempt } => format!("reconnecting({})", attempt),
        ConnectionStatus::Error(_) => "error".into(),
    }
}

impl ConnectionManager {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            connections: DashMap::new(),
            app_handle,
            reconnect_config: ReconnectConfig::default(),
        }
    }

    pub fn create_connection(
        &self,
        id: ConnectionId,
        label: String,
        host: String,
        port: u16,
        user: String,
    ) -> Arc<Connection> {
        let conn = Arc::new(Connection {
            id: id.clone(),
            label,
            host,
            port,
            user,
            status: RwLock::new(ConnectionStatus::Disconnected),
            remote_info: RwLock::new(None),
            reconnect_attempt: RwLock::new(0),
        });
        self.connections.insert(id, conn.clone());
        conn
    }

    pub fn get(&self, id: &str) -> Option<Arc<Connection>> {
        self.connections.get(id).map(|r| r.value().clone())
    }

    pub fn remove(&self, id: &str) {
        self.connections.remove(id);
    }

    /// Calculate the backoff delay for a reconnection attempt.
    pub fn backoff_delay(&self, attempt: u32) -> std::time::Duration {
        let delay = self.reconnect_config.base_delay_ms * 2u64.pow(attempt.min(5));
        let capped = delay.min(self.reconnect_config.max_delay_ms);
        std::time::Duration::from_millis(capped)
    }

    /// Transition a connection to reconnecting state.
    pub async fn start_reconnect(&self, id: &str) -> Option<u32> {
        let conn = self.get(id)?;
        let mut attempt = conn.reconnect_attempt.write().await;
        *attempt += 1;
        if *attempt > self.reconnect_config.max_attempts {
            let mut status = conn.status.write().await;
            *status = ConnectionStatus::Error("Max reconnection attempts exceeded".into());
            return None;
        }
        let mut status = conn.status.write().await;
        *status = ConnectionStatus::Reconnecting { attempt: *attempt };
        Some(*attempt)
    }

    pub async fn list(&self) -> Vec<ConnectionInfo> {
        let mut infos = Vec::new();
        for item in self.connections.iter() {
            let conn = item.value();
            let status = conn.status.read().await;
            let remote = conn.remote_info.read().await;
            infos.push(ConnectionInfo {
                id: conn.id.clone(),
                label: conn.label.clone(),
                host: conn.host.clone(),
                port: conn.port,
                user: conn.user.clone(),
                status: status_to_string(&status),
                error: match &*status {
                    ConnectionStatus::Error(e) => Some(e.clone()),
                    _ => None,
                },
                remote_info: remote.as_ref().map(|r| RemoteInfo {
                    arch: r.arch.clone(),
                    agent_version: r.agent_version.clone().unwrap_or_default(),
                }),
            });
        }
        infos
    }
}
