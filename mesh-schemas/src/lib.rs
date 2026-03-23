//! `mesh-schemas` — well-known schema hashes and routing key constants.
//!
//! Re-exports core schemas from `mesh-core` and adds infrastructure schemas
//! needed by mesh-hub, mesh-client, and other downstream crates.

use std::sync::LazyLock;

use mesh_core::hash::{Hash, schema_hash};
use mesh_core::routing::routing_key;

// Re-export core schemas and routing helper.
pub use mesh_core::routing::routing_key as compute_routing_key;
pub use mesh_core::schema::*;

// ── Infrastructure schemas (PLAN-01) ──

/// Schema hash for `infrastructure/hub` — hub service descriptor.
pub static SCHEMA_HASH_INFRA_HUB: LazyLock<Hash> =
    LazyLock::new(|| schema_hash("infrastructure/hub"));

/// Schema hash for `infrastructure/relay` — relay service descriptor.
pub static SCHEMA_HASH_INFRA_RELAY: LazyLock<Hash> =
    LazyLock::new(|| schema_hash("infrastructure/relay"));

// ── Well-known routing key constants ──

/// Routing key for the `compute` capability namespace.
pub static ROUTING_KEY_COMPUTE: LazyLock<Hash> = LazyLock::new(|| routing_key("compute"));

/// Routing key for the `storage` capability namespace.
pub static ROUTING_KEY_STORAGE: LazyLock<Hash> = LazyLock::new(|| routing_key("storage"));

/// Routing key for `compute/inference`.
pub static ROUTING_KEY_INFERENCE: LazyLock<Hash> =
    LazyLock::new(|| routing_key("compute/inference"));

/// Routing key for the `infrastructure` namespace.
pub static ROUTING_KEY_INFRASTRUCTURE: LazyLock<Hash> =
    LazyLock::new(|| routing_key("infrastructure"));

/// Routing key for `infrastructure/hub` — hub peering discovery.
pub static ROUTING_KEY_INFRASTRUCTURE_HUB: LazyLock<Hash> =
    LazyLock::new(|| routing_key("infrastructure/hub"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infra_schema_hashes_are_unique() {
        assert_ne!(*SCHEMA_HASH_INFRA_HUB, *SCHEMA_HASH_INFRA_RELAY);
        // Also distinct from core schemas
        assert_ne!(*SCHEMA_HASH_INFRA_HUB, *SCHEMA_HASH_CORE_CAPABILITY);
    }

    #[test]
    fn routing_key_constants_are_unique() {
        assert_ne!(*ROUTING_KEY_COMPUTE, *ROUTING_KEY_STORAGE);
        assert_ne!(*ROUTING_KEY_COMPUTE, *ROUTING_KEY_INFERENCE);
        assert_ne!(*ROUTING_KEY_INFRASTRUCTURE, *ROUTING_KEY_INFRASTRUCTURE_HUB);
        assert_ne!(*ROUTING_KEY_INFRASTRUCTURE, *ROUTING_KEY_COMPUTE);
    }

    #[test]
    fn routing_key_constants_match_function() {
        assert_eq!(*ROUTING_KEY_COMPUTE, routing_key("compute"));
        assert_eq!(*ROUTING_KEY_STORAGE, routing_key("storage"));
        assert_eq!(*ROUTING_KEY_INFERENCE, routing_key("compute/inference"));
        assert_eq!(*ROUTING_KEY_INFRASTRUCTURE, routing_key("infrastructure"));
        assert_eq!(
            *ROUTING_KEY_INFRASTRUCTURE_HUB,
            routing_key("infrastructure/hub")
        );
    }
}
