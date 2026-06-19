//! Transport abstraction for communicating with the Remote Agent Host.

use async_trait::async_trait;
use shared_protocol::ProtocolMessage;

pub mod ipc;
#[cfg(feature = "ssh")]
pub mod ssh_channel;

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send(&self, msg: ProtocolMessage) -> anyhow::Result<()>;
    async fn recv(&self) -> anyhow::Result<Option<ProtocolMessage>>;
    fn is_connected(&self) -> bool;
    async fn close(&self) -> anyhow::Result<()>;
}
