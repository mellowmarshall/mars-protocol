//! `mesh-core` — foundational types and operations for the Capability Mesh Protocol.
//!
//! This crate implements the core primitives defined in the
//! [wire specification](../PROTOCOL.md): identity, hashing, descriptors,
//! wire framing, protocol messages, routing, and well-known schema hashes.

pub mod descriptor;
pub mod error;
pub mod frame;
pub mod hash;
pub mod identity;
pub mod message;
pub mod routing;
pub mod schema;

// Re-export key types for convenience.
pub use descriptor::Descriptor;
pub use error::{MeshError, Result};
pub use frame::Frame;
pub use hash::Hash;
pub use identity::{Identity, Keypair};
pub use message::{
    FilterSet, FindNode, FindNodeResult, FindValue, FindValueResult, NodeAddr, NodeInfo, Ping,
    Pong, Store, StoreAck,
};
pub use routing::{hierarchical_routing_keys, routing_key};
pub use schema::{
    SCHEMA_HASH_CORE_CAPABILITY, SCHEMA_HASH_CORE_DISCOVERY_QUERY, SCHEMA_HASH_CORE_KEY_ROTATION,
    SCHEMA_HASH_CORE_RESOLVE, SCHEMA_HASH_CORE_REVOCATION, SCHEMA_HASH_CORE_SCHEMA,
};
