//! Descriptor envelope — the fundamental unit of data on the mesh (Section 2).
//!
//! Everything stored on the DHT is a descriptor: capabilities, schemas,
//! revocations, attestations.

use serde::{Deserialize, Serialize};

use crate::error::{MeshError, Result};
use crate::hash::Hash;
use crate::identity::{Identity, Keypair};

/// Maximum payload size in bytes (64 KB).
pub const MAX_PAYLOAD_SIZE: usize = 65_536;

/// Maximum topic length in bytes.
pub const MAX_TOPIC_SIZE: usize = 255;

/// Maximum number of routing keys per descriptor.
pub const MAX_ROUTING_KEYS: usize = 8;

/// Maximum clock skew tolerance in seconds.
pub const MAX_CLOCK_SKEW_SECS: u64 = 120;

/// Minimum TTL in seconds.
pub const MIN_TTL_SECS: u32 = 60;

/// Maximum TTL in seconds (24 hours).
pub const MAX_TTL_SECS: u32 = 86_400;

/// The descriptor envelope (Section 2).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Descriptor {
    // Immutable header (included in content hash)
    /// Content-hash of the schema this payload conforms to.
    pub schema_hash: Hash,
    /// Publisher-chosen descriptor slot (UTF-8, max 255 bytes).
    pub topic: String,
    /// Opaque payload, interpreted according to schema.
    pub payload: Vec<u8>,

    // Publisher metadata (included in content hash)
    /// Who published this descriptor.
    pub publisher: Identity,
    /// Microseconds since Unix epoch.
    pub timestamp: u64,
    /// Monotonic per publisher, for ordering.
    pub sequence: u64,
    /// Seconds until expiry.
    pub ttl: u32,

    // Routing (included in content hash)
    /// DHT key ranges where this should be stored (max 8).
    pub routing_keys: Vec<Hash>,

    // Derived (not included in content hash)
    /// BLAKE3 of canonical CBOR of all fields above.
    pub id: Hash,
    /// Signature over id.digest by publisher's private key.
    pub signature: Vec<u8>,
}

/// The fields that are included in the content hash computation.
/// Serialized as canonical CBOR with deterministic map key ordering.
#[derive(Serialize)]
struct DescriptorHashInput<'a> {
    payload: &'a [u8],
    publisher: &'a Identity,
    routing_keys: &'a [Hash],
    schema_hash: &'a Hash,
    sequence: u64,
    timestamp: u64,
    topic: &'a str,
    ttl: u32,
}

impl Descriptor {
    /// Compute the content hash for a descriptor's fields (Section 2.1).
    ///
    /// Serializes the hashable fields as canonical CBOR, then computes BLAKE3.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_id(
        schema_hash: &Hash,
        topic: &str,
        payload: &[u8],
        publisher: &Identity,
        timestamp: u64,
        sequence: u64,
        ttl: u32,
        routing_keys: &[Hash],
    ) -> Result<Hash> {
        // Fields in alphabetical key order for deterministic CBOR map
        let input = DescriptorHashInput {
            payload,
            publisher,
            routing_keys,
            schema_hash,
            sequence,
            timestamp,
            topic,
            ttl,
        };
        let mut buf = Vec::new();
        ciborium::into_writer(&input, &mut buf).map_err(|e| MeshError::Cbor(e.to_string()))?;
        Ok(Hash::blake3(&buf))
    }

    /// Create a new descriptor, computing its id and signing it.
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        keypair: &Keypair,
        schema_hash: Hash,
        topic: String,
        payload: Vec<u8>,
        timestamp: u64,
        sequence: u64,
        ttl: u32,
        routing_keys: Vec<Hash>,
    ) -> Result<Self> {
        let publisher = keypair.identity();
        let id = Self::compute_id(
            &schema_hash,
            &topic,
            &payload,
            &publisher,
            timestamp,
            sequence,
            ttl,
            &routing_keys,
        )?;
        let signature = keypair.sign(&id.digest);

        Ok(Self {
            schema_hash,
            topic,
            payload,
            publisher,
            timestamp,
            sequence,
            ttl,
            routing_keys,
            id,
            signature,
        })
    }

    /// Validate a descriptor (Section 2.2, steps 1-7).
    ///
    /// Step 8 (sequence check) requires external state and is not performed here.
    /// Returns `Ok(())` if valid.
    pub fn validate(&self, now_micros: u64) -> Result<()> {
        // Step 1: Recompute id from declared fields
        let computed_id = Self::compute_id(
            &self.schema_hash,
            &self.topic,
            &self.payload,
            &self.publisher,
            self.timestamp,
            self.sequence,
            self.ttl,
            &self.routing_keys,
        )?;
        if computed_id != self.id {
            return Err(MeshError::IdMismatch {
                expected: self.id.to_hex(),
                actual: computed_id.to_hex(),
            });
        }

        // Step 2: Verify signature
        self.publisher.verify(&self.id.digest, &self.signature)?;

        // Step 3: Check expiry — effective_start + ttl > now
        // Step 4 effective timestamp: min(timestamp, now) + ttl to prevent bonus TTL
        let effective_start = std::cmp::min(self.timestamp, now_micros);
        let ttl_micros = u64::from(self.ttl) * 1_000_000;
        if effective_start + ttl_micros <= now_micros {
            return Err(MeshError::Expired);
        }

        // Step 4: Check timestamp not too far in future
        let max_future = now_micros + MAX_CLOCK_SKEW_SECS * 1_000_000;
        if self.timestamp > max_future {
            return Err(MeshError::TimestampFuture {
                max_skew_secs: MAX_CLOCK_SKEW_SECS,
            });
        }

        // TTL bounds check (Section 4.1: min 60s, max 86400s)
        if self.ttl < MIN_TTL_SECS || self.ttl > MAX_TTL_SECS {
            return Err(MeshError::InvalidTtl {
                ttl: self.ttl,
                min: MIN_TTL_SECS,
                max: MAX_TTL_SECS,
            });
        }

        // Step 5: Routing keys non-empty and <= 8
        if self.routing_keys.is_empty() {
            return Err(MeshError::InvalidRoutingKeys {
                reason: "routing_keys must not be empty".into(),
            });
        }
        if self.routing_keys.len() > MAX_ROUTING_KEYS {
            return Err(MeshError::InvalidRoutingKeys {
                reason: format!(
                    "too many routing keys: {} (max {})",
                    self.routing_keys.len(),
                    MAX_ROUTING_KEYS
                ),
            });
        }

        // Step 6: Payload size
        if self.payload.len() > MAX_PAYLOAD_SIZE {
            return Err(MeshError::PayloadTooLarge {
                size: self.payload.len(),
                max: MAX_PAYLOAD_SIZE,
            });
        }

        // Step 7: Topic length
        if self.topic.len() > MAX_TOPIC_SIZE {
            return Err(MeshError::TopicTooLong {
                size: self.topic.len(),
                max: MAX_TOPIC_SIZE,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::schema_hash;
    use crate::routing::routing_key;

    fn now_micros() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }

    fn make_descriptor(kp: &Keypair) -> Descriptor {
        let now = now_micros();
        Descriptor::create(
            kp,
            schema_hash("core/capability"),
            "test-topic".into(),
            b"test payload".to_vec(),
            now,
            1,
            3600,
            vec![routing_key("compute/inference/text-generation")],
        )
        .unwrap()
    }

    #[test]
    fn create_and_validate() {
        let kp = Keypair::generate();
        let desc = make_descriptor(&kp);
        let now = now_micros();
        assert!(desc.validate(now).is_ok());
    }

    #[test]
    fn validate_tampered_payload() {
        let kp = Keypair::generate();
        let mut desc = make_descriptor(&kp);
        desc.payload = b"tampered".to_vec();
        let now = now_micros();
        assert!(matches!(
            desc.validate(now),
            Err(MeshError::IdMismatch { .. })
        ));
    }

    #[test]
    fn validate_wrong_signature() {
        let kp = Keypair::generate();
        let mut desc = make_descriptor(&kp);
        // Corrupt signature
        desc.signature[0] ^= 0xff;
        let now = now_micros();
        assert!(matches!(
            desc.validate(now),
            Err(MeshError::InvalidSignature)
        ));
    }

    #[test]
    fn validate_expired() {
        let kp = Keypair::generate();
        let past = 1_000_000_000_000_000u64; // way in the past
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            b"payload".to_vec(),
            past,
            1,
            60, // 60 second TTL
            vec![routing_key("test")],
        )
        .unwrap();
        let now = now_micros();
        assert!(matches!(desc.validate(now), Err(MeshError::Expired)));
    }

    #[test]
    fn validate_future_timestamp() {
        let kp = Keypair::generate();
        let now = now_micros();
        let future = now + 300 * 1_000_000; // 5 minutes in future
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            b"payload".to_vec(),
            future,
            1,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        assert!(matches!(
            desc.validate(now),
            Err(MeshError::TimestampFuture { .. })
        ));
    }

    #[test]
    fn validate_empty_routing_keys() {
        let kp = Keypair::generate();
        let now = now_micros();
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![], // empty!
        )
        .unwrap();
        assert!(matches!(
            desc.validate(now),
            Err(MeshError::InvalidRoutingKeys { .. })
        ));
    }

    #[test]
    fn validate_too_many_routing_keys() {
        let kp = Keypair::generate();
        let now = now_micros();
        let keys: Vec<Hash> = (0..9).map(|i| routing_key(&format!("key{i}"))).collect();
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            keys,
        )
        .unwrap();
        assert!(matches!(
            desc.validate(now),
            Err(MeshError::InvalidRoutingKeys { .. })
        ));
    }

    #[test]
    fn validate_payload_too_large() {
        let kp = Keypair::generate();
        let now = now_micros();
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            vec![0u8; MAX_PAYLOAD_SIZE + 1],
            now,
            1,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        assert!(matches!(
            desc.validate(now),
            Err(MeshError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn validate_topic_too_long() {
        let kp = Keypair::generate();
        let now = now_micros();
        let long_topic = "x".repeat(256);
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            long_topic,
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        assert!(matches!(
            desc.validate(now),
            Err(MeshError::TopicTooLong { .. })
        ));
    }

    #[test]
    fn content_hash_deterministic() {
        let kp = Keypair::generate();
        let now = now_micros();
        let d1 = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        let d2 = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        assert_eq!(d1.id, d2.id);
    }

    #[test]
    fn borderline_future_timestamp_effective_ttl() {
        // Timestamp slightly in future (within tolerance) — TTL should be
        // computed from min(timestamp, now) + ttl per Section 2.2 step 4
        let kp = Keypair::generate();
        let now = now_micros();
        let slight_future = now + 60 * 1_000_000; // 60s ahead (within 120s skew)
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            b"payload".to_vec(),
            slight_future,
            1,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        // Should be valid because effective_start = min(slight_future, now) = now
        // and now + 3600s > now
        assert!(desc.validate(now).is_ok());
    }

    #[test]
    fn validate_ttl_too_low() {
        let kp = Keypair::generate();
        let now = now_micros();
        // TTL of 30s is below minimum of 60s
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            b"payload".to_vec(),
            now,
            1,
            30,
            vec![routing_key("test")],
        )
        .unwrap();
        assert!(matches!(
            desc.validate(now),
            Err(MeshError::InvalidTtl { .. })
        ));
    }

    #[test]
    fn validate_ttl_too_high() {
        let kp = Keypair::generate();
        let now = now_micros();
        // TTL of 100000s exceeds maximum of 86400s
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            b"payload".to_vec(),
            now,
            1,
            100_000,
            vec![routing_key("test")],
        )
        .unwrap();
        assert!(matches!(
            desc.validate(now),
            Err(MeshError::InvalidTtl { .. })
        ));
    }

    #[test]
    fn validate_ttl_boundary_min() {
        let kp = Keypair::generate();
        let now = now_micros();
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            b"payload".to_vec(),
            now,
            1,
            MIN_TTL_SECS, // exactly 60
            vec![routing_key("test")],
        )
        .unwrap();
        assert!(desc.validate(now).is_ok());
    }

    #[test]
    fn validate_ttl_boundary_max() {
        let kp = Keypair::generate();
        let now = now_micros();
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            b"payload".to_vec(),
            now,
            1,
            MAX_TTL_SECS, // exactly 86400
            vec![routing_key("test")],
        )
        .unwrap();
        assert!(desc.validate(now).is_ok());
    }

    #[test]
    fn validate_forged_id() {
        // Create valid descriptor then swap in a completely fabricated id
        let kp = Keypair::generate();
        let now = now_micros();
        let mut desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        // Replace id with a hash of something else
        desc.id = Hash::blake3(b"forged id");
        // Re-sign with the forged id so signature passes
        desc.signature = kp.sign(&desc.id.digest);
        // Validation should still fail because recomputed id won't match
        assert!(matches!(
            desc.validate(now),
            Err(MeshError::IdMismatch { .. })
        ));
    }

    #[test]
    fn validate_empty_signature() {
        let kp = Keypair::generate();
        let now = now_micros();
        let mut desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        desc.signature = vec![]; // empty signature
        assert!(matches!(
            desc.validate(now),
            Err(MeshError::InvalidSignature)
        ));
    }

    #[test]
    fn validate_topic_at_max_length() {
        let kp = Keypair::generate();
        let now = now_micros();
        // Exactly 255 bytes is valid
        let topic = "x".repeat(255);
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            topic,
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        assert!(desc.validate(now).is_ok());
    }

    #[test]
    fn validate_exactly_8_routing_keys() {
        let kp = Keypair::generate();
        let now = now_micros();
        let keys: Vec<Hash> = (0..8).map(|i| routing_key(&format!("key{i}"))).collect();
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            keys,
        )
        .unwrap();
        assert!(desc.validate(now).is_ok());
    }

    #[test]
    fn validate_payload_at_max_size() {
        let kp = Keypair::generate();
        let now = now_micros();
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "".into(),
            vec![0u8; MAX_PAYLOAD_SIZE], // exactly 65536
            now,
            1,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        assert!(desc.validate(now).is_ok());
    }
}
