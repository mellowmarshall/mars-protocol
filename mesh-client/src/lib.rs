//! `mesh-client` — high-level client API for the Capability Mesh Protocol.
//!
//! Bundles identity, transport, and DHT into a single [`MeshClient`] that
//! handles bootstrap, publish, discover, and ping operations.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;

use mesh_core::frame::{MSG_PING, MSG_PONG, MSG_STORE, MSG_STORE_ACK};
use mesh_core::identity::Keypair;
use mesh_core::message::{NodeAddr, Ping, Pong, Store, StoreAck, from_cbor, to_cbor};
use mesh_core::{Descriptor, Frame, Hash};
use mesh_dht::node::DhtConfig;
use mesh_dht::storage::DescriptorStore;
use mesh_dht::transport::{Transport, TransportError};
use mesh_dht::DhtNode;
use mesh_transport::{MeshConnection, MeshEndpoint};

/// A high-level mesh client that wraps identity, transport, and DHT.
pub struct MeshClient {
    node: DhtNode<DescriptorStore>,
    transport: QuicTransport,
}

impl MeshClient {
    /// Create a new mesh client bound to the given local address.
    pub async fn new(keypair: Keypair, bind_addr: SocketAddr) -> Result<Self, ClientError> {
        let transport = QuicTransport::new(bind_addr, &keypair)?;
        let local_addr = transport.local_addr()?;
        let node_addr = NodeAddr {
            protocol: "quic".into(),
            address: local_addr.to_string(),
        };
        let node = DhtNode::new(keypair, node_addr, DhtConfig::default());
        Ok(Self { node, transport })
    }

    /// Bootstrap the DHT by connecting to seed nodes.
    ///
    /// Returns the number of nodes discovered.
    pub async fn bootstrap(&mut self, seeds: &[NodeAddr]) -> Result<usize, ClientError> {
        self.node
            .bootstrap(seeds, &self.transport)
            .await
            .map_err(ClientError::Transport)
    }

    /// Publish a descriptor to the mesh via a target node.
    ///
    /// Sends a STORE request to the given target address.
    pub async fn publish(
        &mut self,
        descriptor: Descriptor,
        target: &NodeAddr,
    ) -> Result<StoreAck, ClientError> {
        let store = Store {
            sender: self.node.keypair().identity(),
            sender_addr: self.node.addr().clone(),
            descriptor,
        };
        let body = to_cbor(&store).map_err(|e| ClientError::Codec(e.to_string()))?;
        let frame = Frame::new(MSG_STORE, body);

        let resp = self
            .transport
            .send_request(target, frame)
            .await
            .map_err(ClientError::Transport)?;

        if resp.msg_type == MSG_STORE_ACK {
            let ack: StoreAck =
                from_cbor(&resp.body).map_err(|e| ClientError::Codec(e.to_string()))?;
            Ok(ack)
        } else {
            Err(ClientError::UnexpectedResponse(resp.msg_type))
        }
    }

    /// Discover descriptors at a routing key via iterative Kademlia lookup.
    pub async fn discover(
        &mut self,
        routing_key: &Hash,
    ) -> Result<Vec<Descriptor>, ClientError> {
        self.node
            .lookup_value(routing_key, &self.transport)
            .await
            .map_err(ClientError::Transport)
    }

    /// Ping a remote mesh node.
    pub async fn ping(&mut self, addr: &NodeAddr) -> Result<Pong, ClientError> {
        let ping = Ping {
            sender: self.node.keypair().identity(),
            sender_addr: self.node.addr().clone(),
        };
        let body = to_cbor(&ping).map_err(|e| ClientError::Codec(e.to_string()))?;
        let frame = Frame::new(MSG_PING, body);

        let resp = self
            .transport
            .send_request(addr, frame)
            .await
            .map_err(ClientError::Transport)?;

        if resp.msg_type == MSG_PONG {
            let pong: Pong =
                from_cbor(&resp.body).map_err(|e| ClientError::Codec(e.to_string()))?;
            Ok(pong)
        } else {
            Err(ClientError::UnexpectedResponse(resp.msg_type))
        }
    }

    /// This client's identity.
    pub fn identity(&self) -> &mesh_core::identity::Identity {
        self.node.identity()
    }

    /// The local address this client is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr, ClientError> {
        self.transport.local_addr()
    }

    /// Access the underlying DHT node.
    pub fn dht_node(&self) -> &DhtNode<DescriptorStore> {
        &self.node
    }

    /// Mutable access to the underlying DHT node.
    pub fn dht_node_mut(&mut self) -> &mut DhtNode<DescriptorStore> {
        &mut self.node
    }
}

/// Errors from [`MeshClient`] operations.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// Transport-level error (connection, timeout, framing).
    #[error("transport error: {0}")]
    Transport(TransportError),

    /// CBOR encoding/decoding error.
    #[error("codec error: {0}")]
    Codec(String),

    /// Received an unexpected response message type.
    #[error("unexpected response type: 0x{0:02x}")]
    UnexpectedResponse(u8),
}

impl From<TransportError> for ClientError {
    fn from(e: TransportError) -> Self {
        Self::Transport(e)
    }
}

// ── QUIC Transport ──

/// QUIC-based transport that implements the DHT [`Transport`] trait.
///
/// Caches connections to peers to avoid opening a fresh QUIC connection
/// for every request.
struct QuicTransport {
    endpoint: MeshEndpoint,
    connections: Mutex<HashMap<SocketAddr, MeshConnection>>,
}

impl QuicTransport {
    fn new(local_addr: SocketAddr, keypair: &Keypair) -> Result<Self, ClientError> {
        let endpoint = MeshEndpoint::new(local_addr, keypair)
            .map_err(|e| ClientError::Transport(TransportError::ConnectionFailed(e.to_string())))?;
        Ok(Self {
            endpoint,
            connections: Mutex::new(HashMap::new()),
        })
    }

    fn local_addr(&self) -> Result<SocketAddr, ClientError> {
        self.endpoint
            .local_addr()
            .map_err(|e| ClientError::Transport(TransportError::ConnectionFailed(e.to_string())))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_error_display() {
        let err = ClientError::UnexpectedResponse(0xFF);
        assert_eq!(err.to_string(), "unexpected response type: 0xff");
    }

    #[tokio::test]
    async fn smoke_construct_client() {
        let keypair = Keypair::generate();
        let expected_identity = keypair.identity();
        let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
        let client = MeshClient::new(keypair, addr).await.unwrap();
        assert_eq!(client.identity(), &expected_identity);
        assert!(client.local_addr().is_ok());
    }
}
