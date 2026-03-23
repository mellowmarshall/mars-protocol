//! Error types for mesh-core.

use thiserror::Error;

/// Errors that can occur in mesh-core operations.
#[derive(Debug, Error)]
pub enum MeshError {
    /// Hash mismatch during descriptor validation.
    #[error("descriptor id mismatch: expected {expected}, got {actual}")]
    IdMismatch { expected: String, actual: String },

    /// Signature verification failed.
    #[error("invalid signature")]
    InvalidSignature,

    /// Descriptor has expired (timestamp + ttl < now).
    #[error("descriptor expired")]
    Expired,

    /// Descriptor timestamp is too far in the future.
    #[error("descriptor timestamp too far in future (>{max_skew_secs}s ahead)")]
    TimestampFuture { max_skew_secs: u64 },

    /// Routing keys constraint violated.
    #[error("invalid routing keys: {reason}")]
    InvalidRoutingKeys { reason: String },

    /// Payload too large.
    #[error("payload too large: {size} bytes (max {max})")]
    PayloadTooLarge { size: usize, max: usize },

    /// Topic too long.
    #[error("topic too long: {size} bytes (max {max})")]
    TopicTooLong { size: usize, max: usize },

    /// TTL out of valid range.
    #[error("invalid ttl: {ttl}s (must be {min}-{max}s)")]
    InvalidTtl { ttl: u32, min: u32, max: u32 },

    /// Stale descriptor (sequence too low).
    #[error("stale descriptor: sequence {received} < {expected}")]
    StaleSequence { received: u64, expected: u64 },

    /// Unknown signature algorithm.
    #[error("unknown signature algorithm: 0x{0:02x}")]
    UnknownAlgorithm(u8),

    /// Invalid frame magic bytes.
    #[error("invalid frame magic: 0x{0:04x}")]
    InvalidMagic(u16),

    /// Unsupported protocol version.
    #[error("unsupported protocol version: {0}")]
    UnsupportedVersion(u8),

    /// Frame body too large or truncated.
    #[error("frame body error: {0}")]
    FrameBody(String),

    /// CBOR serialization/deserialization error.
    #[error("cbor error: {0}")]
    Cbor(String),

    /// Ed25519 signing error.
    #[error("signing error: {0}")]
    Signing(String),

    /// I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type alias for mesh-core operations.
pub type Result<T> = std::result::Result<T, MeshError>;
