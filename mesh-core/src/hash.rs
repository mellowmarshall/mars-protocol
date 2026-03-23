//! Hash types and BLAKE3 content-addressing.
//!
//! All hashes in the protocol use the `Hash` struct: an algorithm byte
//! followed by the digest bytes (Section 1.2).

use serde::{Deserialize, Serialize};

/// Algorithm ID for BLAKE3 (canonical hash algorithm).
pub const ALG_BLAKE3: u8 = 0x03;

/// Algorithm ID for SHA-256 (interop).
pub const ALG_SHA256: u8 = 0x04;

/// A hash value: algorithm tag + digest bytes.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash {
    /// Algorithm identifier from the Algorithm Registry (Section 1.1).
    pub algorithm: u8,
    /// Raw digest bytes (length determined by algorithm).
    pub digest: Vec<u8>,
}

impl Hash {
    /// Create a new BLAKE3 hash of the given data.
    pub fn blake3(data: &[u8]) -> Self {
        let digest = blake3::hash(data);
        Self {
            algorithm: ALG_BLAKE3,
            digest: digest.as_bytes().to_vec(),
        }
    }

    /// Create a Hash from raw components.
    pub fn new(algorithm: u8, digest: Vec<u8>) -> Self {
        Self { algorithm, digest }
    }

    /// Return the hex-encoded digest.
    pub fn to_hex(&self) -> String {
        hex::encode(&self.digest)
    }

    /// Check if this is a BLAKE3 hash.
    pub fn is_blake3(&self) -> bool {
        self.algorithm == ALG_BLAKE3
    }
}

impl std::fmt::Debug for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Hash(0x{:02x}, {}...)",
            self.algorithm,
            &self.to_hex()[..std::cmp::min(16, self.to_hex().len())]
        )
    }
}

impl std::fmt::Display for Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:02x}:{}", self.algorithm, self.to_hex())
    }
}

/// Compute a well-known schema hash: `BLAKE3("mesh:schema:<name>")`.
pub fn schema_hash(name: &str) -> Hash {
    let input = format!("mesh:schema:{name}");
    Hash::blake3(input.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blake3_deterministic() {
        let h1 = Hash::blake3(b"hello world");
        let h2 = Hash::blake3(b"hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn blake3_different_input() {
        let h1 = Hash::blake3(b"hello");
        let h2 = Hash::blake3(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hash_is_blake3() {
        let h = Hash::blake3(b"test");
        assert!(h.is_blake3());
        assert_eq!(h.algorithm, ALG_BLAKE3);
        assert_eq!(h.digest.len(), 32);
    }

    #[test]
    fn schema_hash_deterministic() {
        let h1 = schema_hash("core/schema");
        let h2 = schema_hash("core/schema");
        assert_eq!(h1, h2);
    }

    #[test]
    fn schema_hash_different_names() {
        let h1 = schema_hash("core/schema");
        let h2 = schema_hash("core/capability");
        assert_ne!(h1, h2);
    }

    #[test]
    fn hex_display() {
        let h = Hash::blake3(b"test");
        let hex_str = h.to_hex();
        assert_eq!(hex_str.len(), 64); // 32 bytes = 64 hex chars
    }
}
