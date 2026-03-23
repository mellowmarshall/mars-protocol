//! QUIC endpoint for mesh nodes.
//!
//! A `MeshEndpoint` wraps a quinn QUIC endpoint configured per Section 8.1:
//! ALPN `mesh/0`, idle timeout 30s, max bidi streams 100, keep-alive 10s.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use quinn::crypto::rustls::QuicServerConfig;
use quinn::{ClientConfig, Endpoint, ServerConfig, TransportConfig, VarInt};
use tracing::{debug, error, info, instrument};

use mesh_core::identity::Keypair;

use crate::connection::{MeshConnection, accept_request};
use crate::error::{Result, TransportError};
use crate::tls;

/// Build the shared transport configuration per Section 8.1.
fn mesh_transport_config() -> TransportConfig {
    let mut transport = TransportConfig::default();
    transport.max_idle_timeout(Some(
        Duration::from_secs(30)
            .try_into()
            .expect("30s fits in IdleTimeout"),
    ));
    transport.keep_alive_interval(Some(Duration::from_secs(10)));
    transport.max_concurrent_bidi_streams(VarInt::from_u32(100));
    transport.max_concurrent_uni_streams(VarInt::from_u32(0));
    // initial_rtt defaults to 333ms in quinn, we set 100ms per spec.
    transport.initial_rtt(Duration::from_millis(100));
    transport
}

/// A mesh QUIC endpoint that can act as both client and server.
pub struct MeshEndpoint {
    endpoint: Endpoint,
}

impl MeshEndpoint {
    /// Create a new mesh endpoint bound to the given address.
    ///
    /// Generates a self-signed TLS certificate derived from the given
    /// mesh keypair and configures QUIC per the protocol spec (Section 8.1).
    #[instrument(skip_all, fields(%addr))]
    pub fn new(addr: SocketAddr, keypair: &Keypair) -> Result<Self> {
        let (cert_chain, key) = tls::generate_self_signed_cert(keypair)?;
        let server_crypto = tls::server_crypto_config(cert_chain, key)?;
        let client_crypto = tls::client_crypto_config()?;

        let transport = Arc::new(mesh_transport_config());

        // Server config
        let mut server_config = ServerConfig::with_crypto(Arc::new(
            QuicServerConfig::try_from(server_crypto)
                .map_err(|e| TransportError::Tls(format!("quic server config: {e}")))?,
        ));
        server_config.transport_config(transport.clone());

        // Client config
        let mut client_config = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)
                .map_err(|e| TransportError::Tls(format!("quic client config: {e}")))?,
        ));
        client_config.transport_config(transport);

        // Create endpoint
        let mut endpoint = Endpoint::server(server_config, addr)?;
        endpoint.set_default_client_config(client_config);

        info!(local = %endpoint.local_addr()?, "mesh endpoint started");
        Ok(Self { endpoint })
    }

    /// The local address this endpoint is bound to.
    pub fn local_addr(&self) -> Result<SocketAddr> {
        Ok(self.endpoint.local_addr()?)
    }

    /// Connect to a remote mesh peer.
    #[instrument(skip(self), fields(%addr))]
    pub async fn connect(&self, addr: SocketAddr) -> Result<MeshConnection> {
        // Use "mesh.local" as server name — we skip TLS verification anyway.
        let connecting = self.endpoint.connect(addr, "mesh.local")?;
        let connection = connecting.await?;
        debug!(peer = %connection.remote_address(), "connected to peer");
        Ok(MeshConnection::new(connection))
    }

    /// Listen for incoming connections and dispatch to a handler.
    ///
    /// For each incoming connection, spawns a tokio task that accepts
    /// bidirectional streams and calls `handler` with each request.
    ///
    /// The handler receives the request frame, a response sender, and the
    /// peer's authenticated mesh identity (extracted from their TLS cert).
    #[instrument(skip_all)]
    pub async fn listen<F, Fut>(&self, handler: F) -> Result<()>
    where
        F: Fn(mesh_core::Frame, crate::connection::ResponseSender, Option<mesh_core::identity::Identity>) -> Fut
            + Send
            + Sync
            + 'static,
        Fut: Future<Output = ()> + Send,
    {
        let handler = Arc::new(handler);
        info!(addr = %self.endpoint.local_addr()?, "listening for connections");

        while let Some(incoming) = self.endpoint.accept().await {
            let handler = handler.clone();
            tokio::spawn(async move {
                match incoming.await {
                    Ok(conn) => {
                        let peer = conn.remote_address();
                        debug!(%peer, "accepted connection");
                        let mesh_conn = MeshConnection::new(conn);
                        // Extract peer's mesh identity from their TLS cert once per connection
                        let peer_identity = mesh_conn.peer_mesh_identity();
                        loop {
                            match mesh_conn.accept_stream().await {
                                Ok((send, recv)) => {
                                    let handler = handler.clone();
                                    let peer_identity = peer_identity.clone();
                                    tokio::spawn(async move {
                                        match accept_request(send, recv).await {
                                            Ok((frame, sender)) => {
                                                handler(frame, sender, peer_identity).await;
                                            }
                                            Err(e) => {
                                                debug!("stream request error: {e}");
                                            }
                                        }
                                    });
                                }
                                Err(_) => {
                                    debug!(%peer, "connection streams exhausted");
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        error!("incoming connection failed: {e}");
                    }
                }
            });
        }
        Ok(())
    }

    /// Close the endpoint.
    pub fn close(&self) {
        self.endpoint.close(VarInt::from_u32(0), b"shutdown");
    }

    /// Wait for all connections to finish.
    pub async fn wait_idle(&self) {
        self.endpoint.wait_idle().await;
    }

    /// Get the underlying quinn endpoint.
    pub fn inner(&self) -> &Endpoint {
        &self.endpoint
    }
}
