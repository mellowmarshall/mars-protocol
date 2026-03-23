//! QUIC transport implementation for mesh-dht's `Transport` trait.
//!
//! Bridges mesh-transport (QUIC) with mesh-dht (Kademlia) by implementing
//! the `Transport` trait using `MeshEndpoint` and `send_request`.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;

use mesh_core::Frame;
use mesh_core::identity::Keypair;
use mesh_core::message::NodeAddr;
use mesh_dht::transport::{Transport, TransportError};
use mesh_transport::{MeshConnection, MeshEndpoint};

/// QUIC-based transport that implements the DHT `Transport` trait.
///
/// Caches connections to peers to avoid opening a fresh QUIC connection
/// for every request (review issue #5).
pub struct QuicTransport {
    endpoint: MeshEndpoint,
    /// Connection cache keyed by socket address.
    connections: Mutex<HashMap<SocketAddr, MeshConnection>>,
}

impl QuicTransport {
    /// Create a new QUIC transport bound to the given local address.
    pub fn new(local_addr: SocketAddr, keypair: &Keypair) -> Result<Self, TransportError> {
        let endpoint = MeshEndpoint::new(local_addr, keypair)
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
        Ok(Self {
            endpoint,
            connections: Mutex::new(HashMap::new()),
        })
    }

    /// The local address this transport is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, TransportError> {
        self.endpoint
            .local_addr()
            .map_err(|e| TransportError::ConnectionFailed(e.to_string()))
    }
}

impl Transport for QuicTransport {
    async fn send_request(&self, addr: &NodeAddr, frame: Frame) -> Result<Frame, TransportError> {
        let socket_addr: SocketAddr = addr.address.parse().map_err(|e| {
            TransportError::Unreachable(format!("bad address '{}': {e}", addr.address))
        })?;

        // Check cache for an existing connection
        let cached = {
            let conns = self.connections.lock().unwrap();
            conns.get(&socket_addr).cloned()
        };

        let conn = if let Some(c) = cached {
            c
        } else {
            let new_conn = self
                .endpoint
                .connect(socket_addr)
                .await
                .map_err(|e| TransportError::ConnectionFailed(e.to_string()))?;
            self.connections
                .lock()
                .unwrap()
                .insert(socket_addr, new_conn.clone());
            new_conn
        };

        match mesh_transport::send_request(&conn, &frame).await {
            Ok(response) => Ok(response),
            Err(e) => {
                // Remove stale connection from cache on error
                self.connections.lock().unwrap().remove(&socket_addr);
                Err(TransportError::FrameError(e.to_string()))
            }
        }
    }
}
