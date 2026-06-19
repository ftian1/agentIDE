//! SSH Channel Transport — stub (requires `ssh` feature).
//!
//! When the `ssh` feature is enabled, this module wraps a `russh::Channel`
//! to implement the `Transport` trait for bidirectional MessagePack framing.

use async_trait::async_trait;
use shared_protocol::ProtocolMessage;
use std::sync::atomic::{AtomicBool, Ordering};

use super::Transport;

/// Stub transport that returns errors until SSH feature is fully implemented.
pub struct SshChannelTransport {
    connected: AtomicBool,
}

impl SshChannelTransport {
    #[allow(unused_variables)]
    pub fn new() -> Self {
        Self {
            connected: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl Transport for SshChannelTransport {
    async fn send(&self, _msg: ProtocolMessage) -> anyhow::Result<()> {
        anyhow::bail!("SSH transport not available (compile with --features ssh)")
    }

    async fn recv(&self) -> anyhow::Result<Option<ProtocolMessage>> {
        Ok(None)
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn close(&self) -> anyhow::Result<()> {
        Ok(())
    }
}
