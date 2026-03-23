//! QUIC transport implementation for mesh-dht's `Transport` trait.
//!
//! Bridges mesh-transport (QUIC) with mesh-dht (Kademlia) by implementing
//! the `Transport` trait using `MeshEndpoint` and `send_request`.

use std::net::SocketAddr;

use mesh_core::Frame;
use mesh_core::message::NodeAddr;
use mesh_dht::transport::{Transport, TransportError};
use mesh_transport::MeshEndpoint;

/// QUIC-based transport that implements the DHT `Transport` trait.
pub struct QuicTransport {
    endpoint: MeshEndpoint,
}

impl QuicTransport {
    /// Create a new QUIC transport bound to the given local address.
    pub fn new(local_addr: SocketAddr) -> Result<Self, TransportError> {
        let endpoint = MeshEndpoint::new(local_addr)
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        Ok(Self { endpoint })
    }
}

impl Transport for QuicTransport {
    async fn send_request(&self, addr: &NodeAddr, frame: Frame) -> Result<Frame, TransportError> {
        let socket_addr: SocketAddr = addr.address.parse().map_err(|e| {
            TransportError::Unreachable(format!("bad address '{}': {e}", addr.address))
        })?;

        let conn = self
            .endpoint
            .connect(socket_addr)
            .await
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;

        let response = mesh_transport::send_request(&conn, &frame)
            .await
            .map_err(|e| TransportError::FrameError(e.to_string()))?;

        Ok(response)
    }
}
