//! SSH Channel Transport — wraps a [`russh::Channel`] to implement the
//! [`Transport`] trait for bidirectional MessagePack framing over an SSH
//! exec/shell channel.
//!
//! Requires the `ssh` feature.
//!
//! ## Architecture: single-actor design
//!
//! The original design used two tasks (reader + writer) sharing
//! `Arc<Mutex<Channel>>`. That deadlocks: the reader holds the mutex across
//! `channel.wait()`, blocking the writer from sending new requests — which
//! are the very thing needed to unblock the reader.
//!
//! The fix is a **single actor task** that owns the channel. `tokio::select!`
//! races the read and write futures, and only one future holds a borrow at
//! a time (the select! drops the other). No mutex, no deadlock.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use russh::{Channel, ChannelMsg, client};
use shared_protocol::{MessageDecoder, ProtocolMessage};
use tokio::sync::{Mutex, mpsc};

use super::Transport;

/// A [`Transport`] that communicates with the Remote Agent Host over an
/// SSH channel.
///
/// The underlying SSH channel is owned by a single background actor task
/// that races reads and writes via `tokio::select!` — no shared mutex,
/// so no deadlock between send and recv.
pub struct SshChannelTransport {
    /// Send half — push framed bytes here to write to the SSH channel.
    write_tx: mpsc::UnboundedSender<Vec<u8>>,
    /// Receive half — poll for decoded protocol messages.
    read_rx: Mutex<mpsc::UnboundedReceiver<ProtocolMessage>>,
    /// Connected flag — shared with the actor task.
    connected: Arc<AtomicBool>,
    /// Background actor task (kept alive).
    _actor: Option<tokio::task::JoinHandle<()>>,
}

impl SshChannelTransport {
    /// Create a new transport wrapping an already-opened SSH channel.
    pub fn new(channel: Channel<client::Msg>) -> Self {
        let connected = Arc::new(AtomicBool::new(true));

        let (write_tx, write_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        let (read_tx, read_rx) = mpsc::unbounded_channel::<ProtocolMessage>();

        let conn = connected.clone();

        // Single actor task — owns the channel, races reads and writes.
        let actor = tokio::spawn(actor_loop(channel, write_rx, read_tx, conn));

        Self {
            write_tx,
            read_rx: Mutex::new(read_rx),
            connected,
            _actor: Some(actor),
        }
    }
}

/// The single-actor event loop. Owns the SSH channel and races
/// reads (incoming data → decode → push to read_rx) against writes
/// (incoming frames from write_rx → send over SSH).
async fn actor_loop(
    mut channel: Channel<client::Msg>,
    mut write_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    read_tx: mpsc::UnboundedSender<ProtocolMessage>,
    connected: Arc<AtomicBool>,
) {
    let mut decoder = MessageDecoder::new();
    let mut decode_count: u64 = 0;

    tracing::info!("SSH channel actor started");

    loop {
        tokio::select! {
            // ── Write path ──────────────────────────────
            frame = write_rx.recv() => {
                match frame {
                    Some(data) => {
                        let bytes: bytes::Bytes = data.into();
                        tracing::debug!(len = bytes.len(), "Actor sending frame to SSH");
                        if let Err(e) = channel.data_bytes(bytes).await {
                            tracing::error!(error = %e, "SSH channel write error, actor dying");
                            break;
                        }
                    }
                    None => {
                        tracing::info!("Write channel closed, actor dying");
                        break;
                    }
                }
            }

            // ── Read path ───────────────────────────────
            msg = channel.wait() => {
                match msg {
                    Some(ChannelMsg::Data { data }) => {
                        decoder.push(&data);
                        loop {
                            match decoder.try_decode() {
                                Ok(Some(msg)) => {
                                    decode_count += 1;
                                    if decode_count <= 10 || decode_count % 100 == 0 {
                                        tracing::info!(kind = msg.kind(), count = decode_count, "Actor decoded message → read_rx");
                                    }
                                    if read_tx.send(msg).is_err() {
                                        tracing::error!("read_tx closed, actor dying");
                                        return; // consumer dropped
                                    }
                                }
                                Ok(None) => break, // need more data
                                Err(e) => {
                                    tracing::error!(error = %e, "SSH channel decode error, actor dying");
                                    return;
                                }
                            }
                        }
                    }
                    Some(ChannelMsg::ExtendedData { data, ext }) => {
                        if ext == 1 {
                            let text = String::from_utf8_lossy(&data);
                            let trimmed = text.trim();
                            if !trimmed.is_empty() {
                                tracing::warn!(stderr = %trimmed, "Agent stderr");
                            }
                        }
                    }
                    Some(ChannelMsg::Eof) | None => {
                        tracing::info!(decoded = decode_count, "SSH channel EOF, actor exiting");
                        break;
                    }
                    _ => {} // ignore other channel messages
                }
            }
        }
    }

    tracing::warn!("SSH channel actor exited, marking disconnected");
    connected.store(false, Ordering::SeqCst);
}

#[async_trait]
impl Transport for SshChannelTransport {
    async fn send(&self, msg: ProtocolMessage) -> anyhow::Result<()> {
        if !self.connected.load(Ordering::SeqCst) {
            anyhow::bail!("SSH channel transport disconnected");
        }
        let frame = shared_protocol::encode(&msg)?;
        tracing::debug!(kind = msg.kind(), len = frame.len(), "Transport send");
        self.write_tx
            .send(frame.to_vec())
            .map_err(|_| anyhow::anyhow!("SSH channel writer disconnected"))?;
        Ok(())
    }

    async fn recv(&self) -> anyhow::Result<Option<ProtocolMessage>> {
        let mut rx = self.read_rx.lock().await;
        match rx.try_recv() {
            Ok(msg) => {
                tracing::trace!(kind = msg.kind(), "Transport recv got message");
                Ok(Some(msg))
            }
            Err(mpsc::error::TryRecvError::Empty) => Ok(None),
            Err(mpsc::error::TryRecvError::Disconnected) => {
                tracing::warn!("Transport recv: read channel disconnected");
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
