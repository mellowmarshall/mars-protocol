//! Routing key computation (Sections 4.4, 4.5).
//!
//! Routing keys map capability type strings to DHT key space positions.

use crate::hash::Hash;

/// Compute a routing key for a capability type string (Section 4.4).
///
/// `routing_key("compute/inference/text-generation")`
/// → `BLAKE3("mesh:route:compute/inference/text-generation")`
pub fn routing_key(type_string: &str) -> Hash {
    let input = format!("mesh:route:{type_string}");
    Hash::blake3(input.as_bytes())
}

/// Compute hierarchical routing keys for a capability path (Section 4.5).
///
/// For `"compute/inference/text-generation"`, returns routing keys for:
/// - `"compute"`
/// - `"compute/inference"`
/// - `"compute/inference/text-generation"`
///
/// This enables both broad and narrow discovery.
pub fn hierarchical_routing_keys(type_path: &str) -> Vec<Hash> {
    let parts: Vec<&str> = type_path.split('/').collect();
    let mut keys = Vec::with_capacity(parts.len());
    for i in 0..parts.len() {
        let prefix = parts[..=i].join("/");
        keys.push(routing_key(&prefix));
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_key_deterministic() {
        let k1 = routing_key("compute/inference/text-generation");
        let k2 = routing_key("compute/inference/text-generation");
        assert_eq!(k1, k2);
    }

    #[test]
    fn routing_key_different_types() {
        let k1 = routing_key("compute/inference/text-generation");
        let k2 = routing_key("storage/object/s3");
        assert_ne!(k1, k2);
    }

    #[test]
    fn routing_key_is_blake3() {
        let k = routing_key("test");
        assert!(k.is_blake3());
        assert_eq!(k.digest.len(), 32);
    }

    #[test]
    fn routing_key_matches_spec_format() {
        // Verify the key is BLAKE3("mesh:route:" || type_string)
        let k = routing_key("compute/inference/text-generation");
        let expected = Hash::blake3(b"mesh:route:compute/inference/text-generation");
        assert_eq!(k, expected);
    }

    #[test]
    fn hierarchical_keys_single_segment() {
        let keys = hierarchical_routing_keys("compute");
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], routing_key("compute"));
    }

    #[test]
    fn hierarchical_keys_three_segments() {
        let keys = hierarchical_routing_keys("compute/inference/text-generation");
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0], routing_key("compute"));
        assert_eq!(keys[1], routing_key("compute/inference"));
        assert_eq!(keys[2], routing_key("compute/inference/text-generation"));
    }

    #[test]
    fn hierarchical_keys_order() {
        let keys = hierarchical_routing_keys("a/b/c/d");
        assert_eq!(keys.len(), 4);
        // Each level should produce a different key
        for i in 0..keys.len() {
            for j in (i + 1)..keys.len() {
                assert_ne!(keys[i], keys[j]);
            }
        }
    }
}
