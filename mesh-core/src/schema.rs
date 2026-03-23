//! Well-known schema hashes (Appendix B).
//!
//! These are computed as `BLAKE3("mesh:schema:<name>")` and are the only
//! hardcoded hashes in the protocol.

use crate::hash::{Hash, schema_hash};
use std::sync::LazyLock;

/// Schema hash for `core/schema` — the self-describing root schema.
pub static SCHEMA_HASH_CORE_SCHEMA: LazyLock<Hash> = LazyLock::new(|| schema_hash("core/schema"));

/// Schema hash for `core/capability` — capability advertisement.
pub static SCHEMA_HASH_CORE_CAPABILITY: LazyLock<Hash> =
    LazyLock::new(|| schema_hash("core/capability"));

/// Schema hash for `core/discovery-query` — discovery query format.
pub static SCHEMA_HASH_CORE_DISCOVERY_QUERY: LazyLock<Hash> =
    LazyLock::new(|| schema_hash("core/discovery-query"));

/// Schema hash for `core/resolve` — resolve request/response.
pub static SCHEMA_HASH_CORE_RESOLVE: LazyLock<Hash> = LazyLock::new(|| schema_hash("core/resolve"));

/// Schema hash for `core/revocation` — descriptor revocation.
pub static SCHEMA_HASH_CORE_REVOCATION: LazyLock<Hash> =
    LazyLock::new(|| schema_hash("core/revocation"));

/// Schema hash for `core/key-rotation` — identity key rotation.
pub static SCHEMA_HASH_CORE_KEY_ROTATION: LazyLock<Hash> =
    LazyLock::new(|| schema_hash("core/key-rotation"));

/// All well-known schema names.
pub const WELL_KNOWN_SCHEMAS: &[&str] = &[
    "core/schema",
    "core/capability",
    "core/discovery-query",
    "core/resolve",
    "core/revocation",
    "core/key-rotation",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_hashes_are_blake3() {
        assert!(SCHEMA_HASH_CORE_SCHEMA.is_blake3());
        assert!(SCHEMA_HASH_CORE_CAPABILITY.is_blake3());
        assert!(SCHEMA_HASH_CORE_DISCOVERY_QUERY.is_blake3());
        assert!(SCHEMA_HASH_CORE_RESOLVE.is_blake3());
        assert!(SCHEMA_HASH_CORE_REVOCATION.is_blake3());
        assert!(SCHEMA_HASH_CORE_KEY_ROTATION.is_blake3());
    }

    #[test]
    fn schema_hashes_are_unique() {
        let hashes: Vec<&Hash> = vec![
            &SCHEMA_HASH_CORE_SCHEMA,
            &SCHEMA_HASH_CORE_CAPABILITY,
            &SCHEMA_HASH_CORE_DISCOVERY_QUERY,
            &SCHEMA_HASH_CORE_RESOLVE,
            &SCHEMA_HASH_CORE_REVOCATION,
            &SCHEMA_HASH_CORE_KEY_ROTATION,
        ];
        for i in 0..hashes.len() {
            for j in (i + 1)..hashes.len() {
                assert_ne!(hashes[i], hashes[j], "schema hashes must be unique");
            }
        }
    }

    #[test]
    fn schema_hashes_deterministic() {
        // Access twice to ensure LazyLock produces same result
        let h1 = SCHEMA_HASH_CORE_SCHEMA.clone();
        let h2 = SCHEMA_HASH_CORE_SCHEMA.clone();
        assert_eq!(h1, h2);
    }

    #[test]
    fn schema_hash_matches_manual() {
        let manual = Hash::blake3(b"mesh:schema:core/schema");
        assert_eq!(*SCHEMA_HASH_CORE_SCHEMA, manual);
    }

    #[test]
    fn well_known_schemas_count() {
        assert_eq!(WELL_KNOWN_SCHEMAS.len(), 6);
    }
}
