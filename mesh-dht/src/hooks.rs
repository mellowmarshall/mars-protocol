//! Protocol hooks for injecting policy into DHT operations.
//!
//! Downstream crates (e.g., mesh-hub) can implement [`ProtocolHook`] to add
//! metering, access control, and audit logging at the DHT protocol layer
//! without modifying `DhtNode`.

use mesh_core::{Descriptor, Hash};

/// Hook trait for intercepting DHT protocol operations.
///
/// All methods have default no-op implementations. Implement only the
/// hooks you need.
pub trait ProtocolHook: Send + Sync {
    /// Called before storing a descriptor. Return `Err` to reject.
    fn pre_store(&self, _descriptor: &Descriptor) -> Result<(), String> {
        Ok(())
    }

    /// Called after a descriptor is successfully stored.
    fn post_store(&self, _descriptor: &Descriptor) {}

    /// Called before executing a query. Return `Err` to reject.
    fn pre_query(&self, _routing_key: &Hash) -> Result<(), String> {
        Ok(())
    }

    /// Called after query results are produced.
    fn post_query(&self, _routing_key: &Hash, _result_count: usize) {}
}

/// A no-op hook implementation (all defaults).
pub struct NoOpHook;

impl ProtocolHook for NoOpHook {}
