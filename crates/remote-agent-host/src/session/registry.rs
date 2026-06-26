//! Session Registry — thread-safe collection of all active sessions.
//!
//! Backed by [`dashmap::DashMap`] for concurrent read/write access.

use dashmap::DashMap;
use std::sync::Arc;

use super::types::{PtyOp, Session};
use shared_protocol::types::*;

/// Concurrent session registry.
pub struct SessionRegistry {
    sessions: DashMap<String, Arc<Session>>,
    max_sessions: usize,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
            max_sessions: 32,
        }
    }

    /// Register a new session. Returns error if the ID already exists
    /// or the max session count is exceeded.
    pub fn register(&self, session: Session) -> Result<Arc<Session>, ErrorCode> {
        let id = session.id.clone();
        if self.sessions.contains_key(&id) {
            return Err(ErrorCode::SessionAlreadyExists);
        }
        if self.sessions.len() >= self.max_sessions {
            return Err(ErrorCode::SpawnFailed);
        }
        let arc = Arc::new(session);
        self.sessions.insert(id, arc.clone());
        Ok(arc)
    }

    /// Get a session by ID.
    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.get(id).map(|r| r.value().clone())
    }

    /// Remove a session by ID, returning it if found.
    pub fn remove(&self, id: &str) -> Option<Arc<Session>> {
        self.sessions.remove(id).map(|(_, v)| v)
    }

    /// Shut down all sessions — send PTY shutdown signal AND kill child processes.
    pub fn shutdown_all(&self) {
        for item in self.sessions.iter() {
            let session = item.value();
            let pid = session.pid;
            tracing::info!(session_id = %session.id, pid = pid, "Shutting down session");
            // Signal the PTY writer thread to stop.
            let _ = session.pty_op_tx.send(PtyOp::Shutdown);
            // Mark as ended.
            let rt = tokio::runtime::Handle::current();
            let _ = rt.block_on(async {
                let mut state = session.state.write().await;
                if !matches!(*state, SessionState::Ended(_)) {
                    *state = SessionState::Ended(EndReason::ConnectionLost);
                }
            });
            // Kill the child CLI process (and its children) so they don't
            // become orphans when the agent exits.
            if pid > 1 {
                // SIGTERM first, then SIGKILL after a short grace period.
                let _ = std::process::Command::new("kill")
                    .arg("-TERM")
                    .arg(pid.to_string())
                    .output();
                std::thread::sleep(std::time::Duration::from_millis(200));
                // Kill entire process group in case CLI spawned subprocesses.
                let _ = std::process::Command::new("kill")
                    .arg("-KILL")
                    .arg(format!("-{}", pid)) // negative PID = process group
                    .output();
                tracing::info!(pid = pid, "Killed child process group");
            }
        }
        self.sessions.clear();
    }
}

impl Default for SessionRegistry {
    fn default() -> Self {
        Self::new()
    }
}
