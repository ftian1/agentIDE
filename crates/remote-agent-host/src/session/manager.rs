//! Session Manager — high-level CRUD operations on the session registry.

use std::sync::Arc;
use tracing;

use super::registry::SessionRegistry;
use super::types::{PtyOp, Session};
use shared_protocol::types::*;

/// Manages session lifecycle with proper error handling.
pub struct SessionManager {
    registry: Arc<SessionRegistry>,
}

impl SessionManager {
    pub fn new(registry: Arc<SessionRegistry>) -> Self {
        Self { registry }
    }

    /// Register a fully-initialized session in the registry.
    pub fn register_session(&self, session: Session) -> Result<Arc<Session>, ErrorCode> {
        tracing::info!(session_id = %session.id, tool = ?session.tool, pid = session.pid, "Registering session");
        self.registry.register(session)
    }

    /// Close a session: send shutdown to PTY loop, remove from registry.
    pub async fn close_session(&self, session_id: &str) -> Result<Option<i32>, ErrorCode> {
        tracing::info!(session_id = %session_id, "Closing session");

        let session = self.registry.get(session_id).ok_or(ErrorCode::SessionNotFound)?;

        // Send shutdown signal
        let _ = session.pty_op_tx.send(PtyOp::Shutdown);

        // Mark as ended
        {
            let mut state = session.state.write().await;
            if !matches!(*state, SessionState::Ended(_)) {
                *state = SessionState::Ended(EndReason::UserClosed);
            }
        }

        // Remove from registry
        self.registry.remove(session_id);

        Ok(None)
    }

    /// Get a session by ID.
    pub fn get(&self, session_id: &str) -> Option<Arc<Session>> {
        self.registry.get(session_id)
    }
}
