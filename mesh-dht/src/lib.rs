//! `mesh-dht` — Kademlia DHT implementation for the Capability Mesh Protocol.
//!
//! Implements Section 4 of the wire specification: XOR distance metrics,
//! k-bucket routing table, descriptor storage, and protocol message handling.

pub mod distance;
pub mod node;
pub mod routing;
pub mod storage;
pub mod transport;

pub use node::DhtNode;
pub use routing::RoutingTable;
pub use storage::DescriptorStore;
