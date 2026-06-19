//! Stdio transport — reads/writes protocol messages over stdin/stdout.
//!
//! The Remote Agent Host is typically invoked by the Desktop Core via SSH:
//! ```text
//! ssh user@host -- ~/.remote-agent-host/agent --mode stdio
//! ```
//! The SSH `exec` channel connects the process's stdin/stdout to the
//! Desktop Core's SSH channel, forming a bidirectional byte pipe.
//!
//! Wire format: `[4-byte BE length][MessagePack ProtocolMessage]`

use shared_protocol::{MessageDecoder, ProtocolMessage};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Run the stdio transport: read from stdin in a loop, decode messages,
/// and send them to the server's message handler channel.
///
/// Returns when stdin is closed (client disconnected).
pub async fn read_loop(
    tx: tokio::sync::mpsc::UnboundedSender<ProtocolMessage>,
) -> anyhow::Result<()> {
    let mut stdin = tokio::io::BufReader::new(tokio::io::stdin());
    let mut decoder = MessageDecoder::new();
    let mut buf = vec![0u8; 65536];

    loop {
        match stdin.read(&mut buf).await {
            Ok(0) => {
                tracing::info!("stdin closed (client disconnected)");
                break;
            }
            Ok(n) => {
                decoder.push(&buf[..n]);

                // Decode all complete frames in the buffer
                loop {
                    match decoder.try_decode() {
                        Ok(Some(msg)) => {
                            tracing::trace!(kind = msg.kind(), "Received message");
                            if tx.send(msg).is_err() {
                                tracing::warn!("Message handler channel closed");
                                return Ok(());
                            }
                        }
                        Ok(None) => {
                            // Need more data
                            break;
                        }
                        Err(e) => {
                            tracing::error!(error = %e, "Decode error");
                            // Send error and continue — don't kill the connection
                            let _ = tx.send(ProtocolMessage::Error {
                                code: shared_protocol::types::ErrorCode::InvalidMessage,
                                message: format!("Decode error: {}", e),
                                session_id: None,
                            });
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "stdin read error");
                return Err(e.into());
            }
        }
    }

    Ok(())
}

/// Write a protocol message to stdout.
pub async fn write_message(msg: &ProtocolMessage) -> anyhow::Result<()> {
    let encoded = shared_protocol::encode(msg)?;
    let mut stdout = tokio::io::stdout();
    stdout.write_all(&encoded).await?;
    stdout.flush().await?;
    Ok(())
}

