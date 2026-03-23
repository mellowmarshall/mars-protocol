//! XOR distance metric for Kademlia routing (Section 4.1–4.3).
//!
//! Node IDs and routing keys live in a 256-bit key space. Distance between
//! two keys is defined as their bitwise XOR, interpreted as an unsigned integer.

use std::cmp::Ordering;

use mesh_core::Hash;

/// XOR two 256-bit hashes, returning the distance as a 32-byte array.
///
/// Both hashes must have 32-byte digests (BLAKE3). Panics if they don't.
pub fn xor_distance(a: &Hash, b: &Hash) -> [u8; 32] {
    debug_assert_eq!(a.digest.len(), 32, "hash a must be 32 bytes");
    debug_assert_eq!(b.digest.len(), 32, "hash b must be 32 bytes");
    let mut result = [0u8; 32];
    for (i, byte) in result.iter_mut().enumerate() {
        *byte = a.digest[i] ^ b.digest[i];
    }
    result
}

/// Count the number of leading zero bits in a 256-bit distance.
///
/// This determines which k-bucket a node falls into relative to the local node.
/// Bucket index = 255 - leading_zeros(distance).
pub fn leading_zeros(distance: &[u8; 32]) -> usize {
    let mut zeros = 0;
    for &byte in distance {
        if byte == 0 {
            zeros += 8;
        } else {
            zeros += byte.leading_zeros() as usize;
            break;
        }
    }
    zeros
}

/// Determine which k-bucket a node belongs to, given the XOR distance to the
/// local node. Returns `None` if the distance is zero (same node).
///
/// Bucket index = 255 - leading_zeros(distance).
/// Bucket 0 = farthest (distance has leading bit set).
/// Bucket 255 = closest (distance has 255 leading zeros, differs only in last bit).
pub fn bucket_index(distance: &[u8; 32]) -> Option<usize> {
    let lz = leading_zeros(distance);
    if lz >= 256 {
        None // distance is zero — same node
    } else {
        Some(255 - lz)
    }
}

/// Compare which of two nodes is closer to a target by XOR distance.
///
/// Returns `Ordering::Less` if `a` is closer, `Greater` if `b` is closer,
/// `Equal` if equidistant.
pub fn distance_cmp(target: &Hash, a: &Hash, b: &Hash) -> Ordering {
    let dist_a = xor_distance(target, a);
    let dist_b = xor_distance(target, b);
    // Compare as big-endian unsigned integers
    dist_a.cmp(&dist_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::Hash;

    fn hash_from_bytes(bytes: [u8; 32]) -> Hash {
        Hash::new(0x03, bytes.to_vec())
    }

    #[test]
    fn xor_distance_same_key() {
        let a = Hash::blake3(b"node1");
        let dist = xor_distance(&a, &a);
        assert_eq!(dist, [0u8; 32]);
    }

    #[test]
    fn xor_distance_symmetric() {
        let a = Hash::blake3(b"node1");
        let b = Hash::blake3(b"node2");
        assert_eq!(xor_distance(&a, &b), xor_distance(&b, &a));
    }

    #[test]
    fn xor_distance_known_values() {
        let a = hash_from_bytes([0xFF; 32]);
        let b = hash_from_bytes([0x00; 32]);
        let dist = xor_distance(&a, &b);
        assert_eq!(dist, [0xFF; 32]);
    }

    #[test]
    fn leading_zeros_all_zero() {
        let dist = [0u8; 32];
        assert_eq!(leading_zeros(&dist), 256);
    }

    #[test]
    fn leading_zeros_first_bit_set() {
        let mut dist = [0u8; 32];
        dist[0] = 0x80;
        assert_eq!(leading_zeros(&dist), 0);
    }

    #[test]
    fn leading_zeros_last_bit_set() {
        let mut dist = [0u8; 32];
        dist[31] = 0x01;
        assert_eq!(leading_zeros(&dist), 255);
    }

    #[test]
    fn leading_zeros_middle() {
        let mut dist = [0u8; 32];
        dist[4] = 0x04; // 00000100 → 5 leading zeros in this byte
        // 4 zero bytes (32 bits) + 5 bits = 37
        assert_eq!(leading_zeros(&dist), 37);
    }

    #[test]
    fn bucket_index_zero_distance() {
        let dist = [0u8; 32];
        assert_eq!(bucket_index(&dist), None);
    }

    #[test]
    fn bucket_index_max_distance() {
        let mut dist = [0u8; 32];
        dist[0] = 0x80; // leading zeros = 0
        assert_eq!(bucket_index(&dist), Some(255));
    }

    #[test]
    fn bucket_index_min_distance() {
        let mut dist = [0u8; 32];
        dist[31] = 0x01; // leading zeros = 255
        assert_eq!(bucket_index(&dist), Some(0));
    }

    #[test]
    fn bucket_index_various() {
        // Bucket index = 255 - leading_zeros
        let mut dist = [0u8; 32];
        dist[0] = 0x01; // leading zeros = 7 → bucket 248
        assert_eq!(bucket_index(&dist), Some(248));

        let mut dist = [0u8; 32];
        dist[1] = 0x80; // 8 + 0 = 8 leading zeros → bucket 247
        assert_eq!(bucket_index(&dist), Some(247));
    }

    #[test]
    fn distance_cmp_closer() {
        let target = Hash::blake3(b"target");
        let a = Hash::blake3(b"close");
        let b = Hash::blake3(b"far");
        // Just verify it returns a valid ordering (we can't predict which is closer
        // without computing, but it should be consistent)
        let cmp1 = distance_cmp(&target, &a, &b);
        let cmp2 = distance_cmp(&target, &b, &a);
        assert_eq!(cmp1, cmp2.reverse());
    }

    #[test]
    fn distance_cmp_same() {
        let target = Hash::blake3(b"target");
        let a = Hash::blake3(b"node");
        assert_eq!(distance_cmp(&target, &a, &a), Ordering::Equal);
    }

    #[test]
    fn distance_cmp_with_self() {
        let target = Hash::blake3(b"target");
        let a = Hash::blake3(b"other");
        // target is distance 0 from itself, should be closer
        assert_eq!(distance_cmp(&target, &target, &a), Ordering::Less);
    }

    #[test]
    fn distance_triangle_consistency() {
        // If A is closer to T than B, and B is closer to T than C,
        // then A is closer to T than C (transitivity).
        let target = Hash::blake3(b"target");
        let nodes: Vec<Hash> = (0..10)
            .map(|i| Hash::blake3(format!("node{i}").as_bytes()))
            .collect();
        let mut sorted = nodes.clone();
        sorted.sort_by(|a, b| distance_cmp(&target, a, b));
        // Verify sorted order is consistent
        for i in 0..sorted.len() - 1 {
            let cmp = distance_cmp(&target, &sorted[i], &sorted[i + 1]);
            assert!(cmp != Ordering::Greater);
        }
    }
}
