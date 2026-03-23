//! Error types for mesh-transport.

use thiserror::Error;

/// Errors that can occur in mesh-transport operations.
#[derive(Debug, Error)]
pub enum TransportError {
    /// QUIC connection error.
    #[error("connection error: {0}")]
    Connection(#[from] quinn::ConnectionError),

    /// QUIC write error.
    #[error("write error: {0}")]
    Write(#[from] quinn::WriteError),

    /// QUIC read error.
    #[error("read to end error: {0}")]
    ReadToEnd(#[from] quinn::ReadToEndError),

    /// QUIC read exact error.
    #[error("read exact error: {0}")]
    ReadExact(#[from] quinn::ReadExactError),

    /// QUIC connect error.
    #[error("connect error: {0}")]
    Connect(#[from] quinn::ConnectError),

    /// TLS/certificate error.
    #[error("tls error: {0}")]
    Tls(String),

    /// Frame protocol error (invalid magic, version, etc.).
    #[error("frame error: {0}")]
    Frame(#[from] mesh_core::MeshError),

    /// I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Stream closed unexpectedly.
    #[error("stream closed unexpectedly")]
    StreamClosed,

    /// Response sender already used.
    #[error("response already sent")]
    AlreadySent,
}

/// Result type alias for mesh-transport.
pub type Result<T> = std::result::Result<T, TransportError>;
