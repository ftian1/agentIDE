//! shared-protocol: Wire protocol types and codec for the Remote AI Agent IDE.
//!
//! This crate defines the message types exchanged between the Desktop Core
//! (Windows Tauri backend) and the Remote Agent Host (Linux daemon) over an
//! SSH stdio channel. Every message is a variant of [`ProtocolMessage`],
//! serialized as length-prefixed MessagePack.
//!
//! # Crate organization
//! - [`types`] — Core enums (`ToolKind`, `SessionState`, `ErrorCode`, etc.) and structs.
//! - [`messages`] — The `ProtocolMessage` enum covering all protocol operations.
//! - [`codec`] — Length-prefixed MessagePack encoder/decoder with buffered streaming.

pub mod codec;
pub mod messages;
pub mod types;

// Re-export key items so consumers only need `use shared_protocol::...`.
pub use codec::{decode, encode, encode_to_vec, MessageDecoder, CodecError, MAX_FRAME_SIZE};
pub use messages::ProtocolMessage;
pub use types::*;
