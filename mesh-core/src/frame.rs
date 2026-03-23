//! Wire frame encoding/decoding (Section 3.2).
//!
//! The frame is a binary envelope around a CBOR-encoded message body.
//! Total header: 24 bytes.

use crate::error::{MeshError, Result};

/// Frame magic bytes: 0x4D48 ("MH" — Mesh).
pub const FRAME_MAGIC: u16 = 0x4D48;

/// Current protocol version.
pub const PROTOCOL_VERSION: u8 = 0x01;

// Message type constants (Section 3.3)
/// PING request (liveness check).
pub const MSG_PING: u8 = 0x01;
/// PONG response (liveness confirmation).
pub const MSG_PONG: u8 = 0x81;
/// STORE request (store a descriptor).
pub const MSG_STORE: u8 = 0x02;
/// STORE_ACK response (acknowledge storage).
pub const MSG_STORE_ACK: u8 = 0x82;
/// FIND_NODE request (find nodes closest to a key).
pub const MSG_FIND_NODE: u8 = 0x03;
/// FIND_NODE_RESULT response (return closest nodes).
pub const MSG_FIND_NODE_RESULT: u8 = 0x83;
/// FIND_VALUE request (find descriptors at a key).
pub const MSG_FIND_VALUE: u8 = 0x04;
/// FIND_VALUE_RESULT response (return descriptors or closer nodes).
pub const MSG_FIND_VALUE_RESULT: u8 = 0x84;

/// A wire protocol frame (Section 3.2).
#[derive(Clone, Debug)]
pub struct Frame {
    /// Magic bytes (must be `FRAME_MAGIC`).
    pub magic: u16,
    /// Protocol version (must be `PROTOCOL_VERSION`).
    pub version: u8,
    /// Message type (one of the `MSG_*` constants).
    pub msg_type: u8,
    /// Random 16-byte request ID for correlation.
    pub msg_id: [u8; 16],
    /// CBOR-encoded message body.
    pub body: Vec<u8>,
}

impl Frame {
    /// Create a new frame with auto-generated msg_id.
    pub fn new(msg_type: u8, body: Vec<u8>) -> Self {
        let mut msg_id = [0u8; 16];
        use rand::RngCore;
        rand::thread_rng().fill_bytes(&mut msg_id);
        Self {
            magic: FRAME_MAGIC,
            version: PROTOCOL_VERSION,
            msg_type,
            msg_id,
            body,
        }
    }

    /// Create a response frame with the same msg_id.
    pub fn response(request: &Frame, msg_type: u8, body: Vec<u8>) -> Self {
        Self {
            magic: FRAME_MAGIC,
            version: PROTOCOL_VERSION,
            msg_type,
            msg_id: request.msg_id,
            body,
        }
    }

    /// Check if this is a request (bit 7 clear) or response (bit 7 set).
    pub fn is_response(&self) -> bool {
        self.msg_type & 0x80 != 0
    }

    /// Serialize the frame to bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let body_len = self.body.len() as u32;
        let mut buf = Vec::with_capacity(24 + self.body.len());
        buf.extend_from_slice(&self.magic.to_be_bytes());
        buf.push(self.version);
        buf.push(self.msg_type);
        buf.extend_from_slice(&self.msg_id);
        buf.extend_from_slice(&body_len.to_be_bytes());
        buf.extend_from_slice(&self.body);
        buf
    }

    /// Deserialize a frame from bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() < 24 {
            return Err(MeshError::FrameBody(format!(
                "frame too short: {} bytes (need at least 24)",
                data.len()
            )));
        }

        let magic = u16::from_be_bytes([data[0], data[1]]);
        if magic != FRAME_MAGIC {
            return Err(MeshError::InvalidMagic(magic));
        }

        let version = data[2];
        if version != PROTOCOL_VERSION {
            return Err(MeshError::UnsupportedVersion(version));
        }

        let msg_type = data[3];
        let mut msg_id = [0u8; 16];
        msg_id.copy_from_slice(&data[4..20]);
        let body_len = u32::from_be_bytes([data[20], data[21], data[22], data[23]]) as usize;

        if data.len() < 24 + body_len {
            return Err(MeshError::FrameBody(format!(
                "frame body truncated: expected {} bytes, got {}",
                body_len,
                data.len() - 24
            )));
        }

        let body = data[24..24 + body_len].to_vec();

        Ok(Self {
            magic,
            version,
            msg_type,
            msg_id,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip() {
        let frame = Frame::new(MSG_PING, b"hello".to_vec());
        let bytes = frame.to_bytes();
        let parsed = Frame::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.magic, FRAME_MAGIC);
        assert_eq!(parsed.version, PROTOCOL_VERSION);
        assert_eq!(parsed.msg_type, MSG_PING);
        assert_eq!(parsed.msg_id, frame.msg_id);
        assert_eq!(parsed.body, b"hello");
    }

    #[test]
    fn frame_response() {
        let req = Frame::new(MSG_PING, vec![]);
        let resp = Frame::response(&req, MSG_PONG, b"pong".to_vec());
        assert_eq!(resp.msg_id, req.msg_id);
        assert_eq!(resp.msg_type, MSG_PONG);
        assert!(resp.is_response());
        assert!(!req.is_response());
    }

    #[test]
    fn frame_invalid_magic() {
        let mut bytes = Frame::new(MSG_PING, vec![]).to_bytes();
        bytes[0] = 0xFF;
        assert!(matches!(
            Frame::from_bytes(&bytes),
            Err(MeshError::InvalidMagic(_))
        ));
    }

    #[test]
    fn frame_unsupported_version() {
        let mut bytes = Frame::new(MSG_PING, vec![]).to_bytes();
        bytes[2] = 0x99;
        assert!(matches!(
            Frame::from_bytes(&bytes),
            Err(MeshError::UnsupportedVersion(0x99))
        ));
    }

    #[test]
    fn frame_truncated() {
        let bytes = Frame::new(MSG_PING, b"hello".to_vec()).to_bytes();
        // Truncate the body
        assert!(Frame::from_bytes(&bytes[..25]).is_err());
    }

    #[test]
    fn frame_too_short() {
        assert!(Frame::from_bytes(&[0u8; 10]).is_err());
    }

    #[test]
    fn frame_header_size() {
        let frame = Frame::new(MSG_PING, vec![]);
        let bytes = frame.to_bytes();
        assert_eq!(bytes.len(), 24); // header only, empty body
    }

    #[test]
    fn message_type_response_bit() {
        assert_eq!(MSG_PING | 0x80, MSG_PONG);
        assert_eq!(MSG_STORE | 0x80, MSG_STORE_ACK);
        assert_eq!(MSG_FIND_NODE | 0x80, MSG_FIND_NODE_RESULT);
        assert_eq!(MSG_FIND_VALUE | 0x80, MSG_FIND_VALUE_RESULT);
    }

    #[test]
    fn frame_empty_body() {
        let frame = Frame::new(MSG_PONG, vec![]);
        let bytes = frame.to_bytes();
        let parsed = Frame::from_bytes(&bytes).unwrap();
        assert!(parsed.body.is_empty());
    }

    #[test]
    fn frame_large_body() {
        let body = vec![0xAB; 10_000];
        let frame = Frame::new(MSG_STORE, body.clone());
        let bytes = frame.to_bytes();
        let parsed = Frame::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.body, body);
    }

    #[test]
    fn frame_body_len_exceeds_data() {
        // Craft a frame where body_len claims more data than provided
        let mut buf = Vec::new();
        buf.extend_from_slice(&FRAME_MAGIC.to_be_bytes());
        buf.push(PROTOCOL_VERSION);
        buf.push(MSG_PING);
        buf.extend_from_slice(&[0u8; 16]); // msg_id
        buf.extend_from_slice(&1000u32.to_be_bytes()); // claims 1000 bytes
        buf.extend_from_slice(&[0u8; 10]); // only 10 bytes
        assert!(Frame::from_bytes(&buf).is_err());
    }

    #[test]
    fn frame_valid_msg_types() {
        // All request types have bit 7 clear
        for &t in &[MSG_PING, MSG_STORE, MSG_FIND_NODE, MSG_FIND_VALUE] {
            assert_eq!(
                t & 0x80,
                0,
                "request type 0x{t:02x} should have bit 7 clear"
            );
        }
        // All response types have bit 7 set
        for &t in &[
            MSG_PONG,
            MSG_STORE_ACK,
            MSG_FIND_NODE_RESULT,
            MSG_FIND_VALUE_RESULT,
        ] {
            assert_ne!(t & 0x80, 0, "response type 0x{t:02x} should have bit 7 set");
        }
    }

    #[test]
    fn frame_zero_body_len() {
        let frame = Frame::new(MSG_PING, vec![]);
        let bytes = frame.to_bytes();
        // Verify body_len field is 0
        let body_len = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
        assert_eq!(body_len, 0);
        let parsed = Frame::from_bytes(&bytes).unwrap();
        assert!(parsed.body.is_empty());
    }
}
