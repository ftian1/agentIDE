//! Length-prefixed MessagePack codec for framing protocol messages
//! over a bidirectional byte stream (SSH stdio channel, TCP socket, etc.).
//!
//! Wire format: `[4-byte big-endian payload length][MessagePack payload]`
//!
//! Max frame size is 16 MiB enforced on both encode and decode sides.

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::{BufMut, BytesMut};
use std::io;

use crate::messages::ProtocolMessage;

/// Maximum size of a single protocol frame (16 MiB).
pub const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// Errors that can occur during encode or decode.
#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("frame exceeds maximum size: {0} bytes")]
    FrameTooLarge(usize),

    #[error("serialization error: {0}")]
    Serialize(#[from] rmp_serde::encode::Error),

    #[error("deserialization error: {0}")]
    Deserialize(#[from] rmp_serde::decode::Error),

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("incomplete frame (expected {expected}, got {got})")]
    Incomplete { expected: usize, got: usize },

    #[error("connection closed")]
    ConnectionClosed,
}

/// Encode a `ProtocolMessage` into a length-prefixed buffer.
///
/// Returns a `BytesMut` containing `[4-byte len][msgpack bytes]`.
pub fn encode(msg: &ProtocolMessage) -> Result<BytesMut, CodecError> {
    let payload = rmp_serde::to_vec(msg)?;
    let payload_len = payload.len();

    if payload_len > MAX_FRAME_SIZE {
        return Err(CodecError::FrameTooLarge(payload_len));
    }

    let mut buf = BytesMut::with_capacity(4 + payload_len);
    buf.put_u32(payload_len as u32);
    buf.put_slice(&payload);

    Ok(buf)
}

/// Encode a `ProtocolMessage` into a `Vec<u8>`.
pub fn encode_to_vec(msg: &ProtocolMessage) -> Result<Vec<u8>, CodecError> {
    let payload = rmp_serde::to_vec(msg)?;
    let payload_len = payload.len();

    if payload_len > MAX_FRAME_SIZE {
        return Err(CodecError::FrameTooLarge(payload_len));
    }

    let mut out = Vec::with_capacity(4 + payload_len);
    out.write_u32::<BigEndian>(payload_len as u32)?;
    out.extend_from_slice(&payload);

    Ok(out)
}

/// Attempt to decode one `ProtocolMessage` from a buffer.
///
/// Returns `None` if the buffer doesn't yet contain a complete frame.
/// On success, returns the decoded message and the number of bytes consumed.
pub fn decode(buf: &[u8]) -> Result<Option<(ProtocolMessage, usize)>, CodecError> {
    if buf.len() < 4 {
        return Ok(None);
    }

    let mut len_bytes = &buf[..4];
    let payload_len = len_bytes.read_u32::<BigEndian>()? as usize;

    if payload_len > MAX_FRAME_SIZE {
        return Err(CodecError::FrameTooLarge(payload_len));
    }

    if buf.len() < 4 + payload_len {
        return Ok(None);
    }

    let payload = &buf[4..4 + payload_len];
    let msg: ProtocolMessage = rmp_serde::from_slice(payload)?;
    let consumed = 4 + payload_len;

    Ok(Some((msg, consumed)))
}

/// A buffered decoder for reading frames from an async stream.
///
/// Call `push()` with raw bytes as they arrive, then `try_decode()`
/// to extract complete messages. Suitable for use in a read loop.
pub struct MessageDecoder {
    buf: BytesMut,
}

impl MessageDecoder {
    pub fn new() -> Self {
        Self {
            buf: BytesMut::with_capacity(65536),
        }
    }

    /// Append received bytes to the internal buffer.
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Try to decode one complete message from the buffer.
    ///
    /// Returns `Some(msg)` if a complete frame was found (consumed from buffer).
    /// Returns `None` if more data is needed.
    /// Returns an error if the frame is malformed or too large.
    pub fn try_decode(&mut self) -> Result<Option<ProtocolMessage>, CodecError> {
        match decode(&self.buf)? {
            Some((msg, consumed)) => {
                // Advance buffer past the consumed bytes
                let _ = self.buf.split_to(consumed);
                Ok(Some(msg))
            }
            None => Ok(None),
        }
    }

    /// Returns the number of buffered (unconsumed) bytes.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Returns true if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

impl Default for MessageDecoder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::ProtocolMessage;

    #[test]
    fn test_encode_decode_roundtrip() {
        let msg = ProtocolMessage::TerminalData {
            session_id: "test-session".into(),
            data: b"Hello, world!\x1b[0m".to_vec(),
            seq: 42,
        };

        let encoded = encode(&msg).unwrap();
        let (decoded, consumed) = decode(&encoded).unwrap().unwrap();

        assert_eq!(consumed, encoded.len());
        match decoded {
            ProtocolMessage::TerminalData { session_id, data, seq } => {
                assert_eq!(session_id, "test-session");
                assert_eq!(data, b"Hello, world!\x1b[0m");
                assert_eq!(seq, 42);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn test_decoder_buffering() {
        let msg = ProtocolMessage::Ping { nonce: 99 };
        let encoded = encode_to_vec(&msg).unwrap();

        let mut decoder = MessageDecoder::new();

        // Push partial data (first 2 bytes only)
        decoder.push(&encoded[..2]);
        assert!(decoder.try_decode().unwrap().is_none());

        // Push remaining bytes
        decoder.push(&encoded[2..]);
        let decoded = decoder.try_decode().unwrap().unwrap();
        match decoded {
            ProtocolMessage::Ping { nonce } => assert_eq!(nonce, 99),
            _ => panic!("wrong variant"),
        }

        // Buffer should be empty after consuming
        assert!(decoder.is_empty());
    }

    #[test]
    fn test_multiple_frames() {
        let msg1 = ProtocolMessage::Ping { nonce: 1 };
        let msg2 = ProtocolMessage::Pong { nonce: 2 };

        let mut buf = Vec::new();
        buf.extend_from_slice(&encode_to_vec(&msg1).unwrap());
        buf.extend_from_slice(&encode_to_vec(&msg2).unwrap());

        let (decoded1, consumed1) = decode(&buf).unwrap().unwrap();
        assert!(matches!(decoded1, ProtocolMessage::Ping { nonce: 1 }));

        let (decoded2, consumed2) = decode(&buf[consumed1..]).unwrap().unwrap();
        assert!(matches!(decoded2, ProtocolMessage::Pong { nonce: 2 }));

        assert_eq!(consumed1 + consumed2, buf.len());
    }

    #[test]
    fn test_frame_too_large() {
        // We can't actually allocate 16MB+ in a test, so just verify
        // the check is wired up by testing a reasonable boundary.
        let msg = ProtocolMessage::TerminalData {
            session_id: "s".into(),
            data: vec![0u8; MAX_FRAME_SIZE + 1],
            seq: 0,
        };
        assert!(matches!(
            encode(&msg).unwrap_err(),
            CodecError::FrameTooLarge(_)
        ));
    }

    #[test]
    fn test_incomplete_header() {
        let buf = vec![0x00, 0x01]; // only 2 bytes of the 4-byte length
        assert!(decode(&buf).unwrap().is_none());
    }

    #[test]
    fn test_incomplete_body() {
        let msg = ProtocolMessage::Ping { nonce: 777 };
        let mut encoded = encode_to_vec(&msg).unwrap();
        // Truncate the last byte
        let original_len = encoded.len();
        encoded.truncate(original_len - 1);

        // The header is complete, but the body is not
        assert!(decode(&encoded).unwrap().is_none());
    }
}
