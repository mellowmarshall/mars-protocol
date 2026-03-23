//! `mesh-transport` — QUIC transport layer for the Capability Mesh Protocol.
//!
//! Provides QUIC-based networking for mesh nodes, implementing Section 8 of
//! the wire specification. Handles connection management, frame send/receive
//! over QUIC streams, and a high-level request/response API.
//!
//! # Architecture
//!
//! - [`MeshEndpoint`] — QUIC endpoint that acts as both client and server
//! - [`MeshConnection`] — wrapper around a QUIC connection to a peer
//! - [`send_request`] — send a request frame and receive the response
//! - [`accept_request`] — accept an incoming request and return a response sender
//! - [`ResponseSender`] — one-shot sender for writing a response frame

pub mod connection;
pub mod endpoint;
pub mod error;
pub mod tls;

// Re-export key types.
pub use connection::{
    MeshConnection, ResponseSender, accept_request, recv_frame, send_frame, send_request,
};
pub use endpoint::MeshEndpoint;
pub use error::{Result, TransportError};

#[cfg(test)]
mod tests;
