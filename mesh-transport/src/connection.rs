//! Connection management for mesh QUIC transport.

use mesh_core::Frame;
use mesh_core::identity::Identity;
use quinn::{Connection, RecvStream, SendStream};
use tracing::{debug, instrument};

use crate::error::{Result, TransportError};
use crate::tls;

/// Maximum frame body size (1 MB per Section 8.1 max stream data).
const MAX_FRAME_BODY: usize = 1_048_576;

/// A wrapper around a QUIC connection to a mesh peer.
#[derive(Clone, Debug)]
pub struct MeshConnection {
    inner: Connection,
}

impl MeshConnection {
    /// Wrap an established QUIC connection.
    pub fn new(inner: Connection) -> Self {
        Self { inner }
    }

    /// The peer's address.
    pub fn remote_address(&self) -> std::net::SocketAddr {
        self.inner.remote_address()
    }

    /// Open a bidirectional stream for a request-response exchange.
    pub async fn open_stream(&self) -> Result<(SendStream, RecvStream)> {
        let (send, recv) = self.inner.open_bi().await?;
        Ok((send, recv))
    }

    /// Accept an incoming bidirectional stream.
    pub async fn accept_stream(&self) -> Result<(SendStream, RecvStream)> {
        let (send, recv) = self.inner.accept_bi().await?;
        Ok((send, recv))
    }

    /// Close the connection.
    pub fn close(&self, reason: &str) {
        self.inner
            .close(quinn::VarInt::from_u32(0), reason.as_bytes());
    }

    /// Extract the peer's mesh [`Identity`] from their TLS certificate.
    ///
    /// After the QUIC handshake, the peer's Ed25519 public key is extracted
    /// from their self-signed TLS certificate. This IS their mesh identity —
    /// no additional authentication is needed because TLS already proved they
    /// hold the corresponding private key.
    pub fn peer_mesh_identity(&self) -> Option<Identity> {
        let certs = self.inner.peer_identity()?;
        let certs = certs.downcast::<Vec<rustls::pki_types::CertificateDer<'static>>>().ok()?;
        let first_cert = certs.first()?;
        tls::identity_from_cert_der(first_cert.as_ref())
    }

    /// Get the underlying quinn connection.
    pub fn inner(&self) -> &Connection {
        &self.inner
    }
}

/// Send a [`Frame`] over a QUIC send stream.
///
/// Writes the full binary frame (header + body) and finishes the send side.
#[instrument(skip_all, fields(msg_type = frame.msg_type))]
pub async fn send_frame(send: &mut SendStream, frame: &Frame) -> Result<()> {
    let bytes = frame.to_bytes();
    send.write_all(&bytes).await?;
    send.finish().map_err(|_| TransportError::StreamClosed)?;
    debug!(len = bytes.len(), "sent frame");
    Ok(())
}

/// Receive a [`Frame`] from a QUIC receive stream.
///
/// Reads the 24-byte header, validates magic/version, reads the body,
/// and returns the parsed frame.
#[instrument(skip_all)]
pub async fn recv_frame(recv: &mut RecvStream) -> Result<Frame> {
    // Read the 24-byte header.
    let mut header = [0u8; 24];
    recv.read_exact(&mut header)
        .await
        .map_err(TransportError::ReadExact)?;

    // Validate magic and version from header.
    let magic = u16::from_be_bytes([header[0], header[1]]);
    if magic != mesh_core::frame::FRAME_MAGIC {
        return Err(TransportError::Frame(mesh_core::MeshError::InvalidMagic(
            magic,
        )));
    }
    let version = header[2];
    if version != mesh_core::frame::PROTOCOL_VERSION {
        return Err(TransportError::Frame(
            mesh_core::MeshError::UnsupportedVersion(version),
        ));
    }

    // Parse body length from header.
    let body_len = u32::from_be_bytes([header[20], header[21], header[22], header[23]]) as usize;
    if body_len > MAX_FRAME_BODY {
        return Err(TransportError::Frame(mesh_core::MeshError::FrameBody(
            format!("body too large: {body_len} bytes (max {MAX_FRAME_BODY})"),
        )));
    }

    // Read the body.
    let mut body = vec![0u8; body_len];
    if body_len > 0 {
        recv.read_exact(&mut body)
            .await
            .map_err(TransportError::ReadExact)?;
    }

    // Reconstruct full bytes and parse.
    let mut full = Vec::with_capacity(24 + body_len);
    full.extend_from_slice(&header);
    full.extend_from_slice(&body);
    let frame = Frame::from_bytes(&full)?;

    debug!(msg_type = frame.msg_type, body_len, "received frame");
    Ok(frame)
}

/// High-level: send a request frame and receive the response on a new stream.
///
/// Opens a bidirectional stream, sends the request frame, receives the
/// response frame, and returns it. The stream is consumed (finished/closed).
#[instrument(skip_all, fields(msg_type = request.msg_type, peer = %conn.remote_address()))]
pub async fn send_request(conn: &MeshConnection, request: &Frame) -> Result<Frame> {
    let (mut send, mut recv) = conn.open_stream().await?;
    send_frame(&mut send, request).await?;
    let response = recv_frame(&mut recv).await?;
    Ok(response)
}

/// A sender for writing a response frame back on an accepted stream.
///
/// Ensures at most one response is sent per stream.
pub struct ResponseSender {
    send: Option<SendStream>,
}

impl ResponseSender {
    fn new(send: SendStream) -> Self {
        Self { send: Some(send) }
    }

    /// Send the response frame. Consumes the sender.
    pub async fn send(mut self, frame: &Frame) -> Result<()> {
        let mut send = self.send.take().ok_or(TransportError::AlreadySent)?;
        send_frame(&mut send, frame).await
    }
}

/// High-level: accept an incoming request from a bidirectional stream.
///
/// Reads the request frame and returns it along with a [`ResponseSender`]
/// that can be used to write the response.
#[instrument(skip_all)]
pub async fn accept_request(
    send: SendStream,
    mut recv: RecvStream,
) -> Result<(Frame, ResponseSender)> {
    let frame = recv_frame(&mut recv).await?;
    Ok((frame, ResponseSender::new(send)))
}
