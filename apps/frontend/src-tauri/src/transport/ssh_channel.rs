//! SSH Channel Transport — wraps a [`russh::Channel`] to implement the
//! [`Transport`] trait for bidirectional MessagePack framing over an SSH
//! exec/shell channel.
//!
//! Requires the `ssh` feature.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use russh::ChannelMsg;
use shared_protocol::{MessageDecoder, ProtocolMessage};
use tokio::sync::{Mutex, mpsc};

use super::Transport;

/// A [`Transport`] that communicates with the Remote Agent Host over an
/// SSH channel.
///
/// The underlying [`russh::Channel`] requires `&mut self` for operations,
/// so we share it behind `Arc<Mutex<Channel>>` between reader and writer
/// background tasks.
pub struct SshChannelTransport {
    /// Send half — push framed bytes here to write to the SSH channel.
    write_tx: mpsc::UnboundedSender<Vec<u8>>,
    /// Receive half — poll for decoded protocol messages.
    read_rx: Mutex<mpsc::UnboundedReceiver<ProtocolMessage>>,
    /// Connected flag.
    connected: AtomicBool,
    /// Background tasks (kept alive).
    _tasks: Option<(tokio::task::JoinHandle<()>, tokio::task::JoinHandle<()>)>,
}

impl SshChannelTransport {
    /// Create a new transport wrapping an already-opened SSH channel.
    pub fn new(channel: russh::Channel<russh::client::Msg>) -> Self {
        let ch = Arc::new(Mutex::new(channel));
        let connected = Arc::new(AtomicBool::new(true));

        let (write_tx, mut write_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (read_tx, read_rx) = mpsc::unbounded_channel::<ProtocolMessage>();

        let conn_w = connected.clone();
        let conn_r = connected.clone();
        let ch_w = ch.clone();
        let ch_r = ch;

        // Writer task
        let writer = tokio::spawn(async move {
            while conn_w.load(Ordering::SeqCst) {
                match write_rx.recv().await {
                    Some(data) => {
                        let bytes: bytes::Bytes = data.into();
                        let guard = ch_w.lock().await;
                        if guard.data_bytes(bytes).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            conn_w.store(false, Ordering::SeqCst);
        });

        // Reader task
        let reader = tokio::spawn(async move {
            let mut decoder = MessageDecoder::new();
            while conn_r.load(Ordering::SeqCst) {
                let msg = {
                    let mut guard = ch_r.lock().await;
                    guard.wait().await
                };
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        decoder.push(&data);
                        loop {
                            match decoder.try_decode() {
                                Ok(Some(msg)) => {
                                    if read_tx.send(msg).is_err() {
                                        return;
                                    }
                                }
                                Ok(None) => break,
                                Err(e) => {
                                    tracing::error!(error = %e, "SSH channel decode error");
                                    return;
                                }
                            }
                        }
                    }
                    Some(ChannelMsg::Eof) | None => break,
                    _ => continue,
                }
            }
            conn_r.store(false, Ordering::SeqCst);
        });

        Self {
            write_tx,
            read_rx: Mutex::new(read_rx),
            connected: AtomicBool::new(true),
            _tasks: Some((writer, reader)),
        }
    }
}

#[async_trait]
impl Transport for SshChannelTransport {
    async fn send(&self, msg: ProtocolMessage) -> anyhow::Result<()> {
        if !self.connected.load(Ordering::SeqCst) {
            anyhow::bail!("SSH channel transport disconnected");
        }
        let frame = shared_protocol::encode(&msg)?;
        self.write_tx
            .send(frame.to_vec())
            .map_err(|_| anyhow::anyhow!("SSH channel writer disconnected"))?;
        Ok(())
    }

    async fn recv(&self) -> anyhow::Result<Option<ProtocolMessage>> {
        if !self.connected.load(Ordering::SeqCst) {
            return Ok(None);
        }
        let mut rx = self.read_rx.lock().await;
        match rx.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                self.connected.store(false, Ordering::SeqCst);
                Ok(None)
            }
        }
    }

    fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    async fn close(&self) -> anyhow::Result<()> {
        self.connected.store(false, Ordering::SeqCst);
        Ok(())
    }
}
