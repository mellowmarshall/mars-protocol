//! Transport trait abstraction for DHT network communication.
//!
//! Defines the `Transport` trait so the DHT logic can be tested with mocks
//! while the real QUIC transport (mesh-transport) is built in parallel.

use std::future::Future;

use mesh_core::Frame;
use mesh_core::message::NodeAddr;

/// Errors from transport operations.
#[derive(Debug, thiserror::Error)]
pub enum TransportError {
    /// Failed to establish a connection to the peer.
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
    /// The request timed out waiting for a response.
    #[error("request timed out")]
    Timeout,
    /// Error encoding or decoding a protocol frame.
    #[error("frame error: {0}")]
    FrameError(String),
    /// The peer address could not be reached.
    #[error("peer unreachable: {0}")]
    Unreachable(String),
}

/// Abstraction over the network transport layer.
///
/// The DHT logic uses this trait to send requests to peers. The real
/// implementation will use QUIC (mesh-transport), but tests use a mock.
pub trait Transport: Send + Sync {
    /// Send a request frame to a peer and wait for the response.
    fn send_request(
        &self,
        addr: &NodeAddr,
        frame: Frame,
    ) -> impl Future<Output = Result<Frame, TransportError>> + Send;
}
