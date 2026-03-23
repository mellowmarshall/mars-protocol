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

// DescriptorHashInput removed: canonical CBOR is now built explicitly
// via compute_id() using BTreeMap<String, ciborium::Value> to guarantee
// interoperable, byte-identical serialization across implementations.

impl Descriptor {
    /// Compute the content hash for a descriptor's fields (Section 2.1).
    ///
    /// Builds an explicit CBOR map with lexicographically sorted string keys
    /// and precisely typed values, then hashes with BLAKE3. This canonical
    /// form is used ONLY for content-hashing — NOT for network serialization.
    /// See PROTOCOL.md Appendix C for the specification and test vectors.
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
        let buf = Self::canonical_cbor_bytes(
            schema_hash,
            topic,
            payload,
            publisher,
            timestamp,
            sequence,
            ttl,
            routing_keys,
        );
        Ok(Hash::blake3(&buf))
    }

    /// Produce the canonical CBOR bytes for content-hashing.
    ///
    /// The output is a CBOR map (major type 5) with string keys in
    /// lexicographic order. Each value uses the exact CBOR type specified
    /// in the protocol (see Appendix C).
    #[allow(clippy::too_many_arguments)]
    fn canonical_cbor_bytes(
        schema_hash: &Hash,
        topic: &str,
        payload: &[u8],
        publisher: &Identity,
        timestamp: u64,
        sequence: u64,
        ttl: u32,
        routing_keys: &[Hash],
    ) -> Vec<u8> {
        use ciborium::Value;
        use std::collections::BTreeMap;

        let mut map = BTreeMap::new();
        map.insert("payload", Value::Bytes(payload.to_vec()));
        map.insert(
            "publisher",
            Value::Array(vec![
                Value::Integer(publisher.algorithm.into()),
                Value::Bytes(publisher.public_key.clone()),
            ]),
        );
        map.insert(
            "routing_keys",
            Value::Array(
                routing_keys
                    .iter()
                    .map(|h| {
                        Value::Array(vec![
                            Value::Integer(h.algorithm.into()),
                            Value::Bytes(h.digest.clone()),
                        ])
                    })
                    .collect(),
            ),
        );
        map.insert(
            "schema_hash",
            Value::Array(vec![
                Value::Integer(schema_hash.algorithm.into()),
                Value::Bytes(schema_hash.digest.clone()),
            ]),
        );
        map.insert("sequence", Value::Integer(sequence.into()));
        map.insert("timestamp", Value::Integer(timestamp.into()));
        map.insert("topic", Value::Text(topic.to_string()));
        map.insert("ttl", Value::Integer(ttl.into()));

        let cbor_value = Value::Map(
            map.into_iter()
                .map(|(k, v)| (Value::Text(k.into()), v))
                .collect(),
        );

        let mut buf = Vec::new();
        ciborium::into_writer(&cbor_value, &mut buf)
            .expect("CBOR serialization cannot fail for well-formed values");
        buf
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

#[cfg(test)]
mod test_vectors {
    use super::*;
    use crate::hash::Hash;
    use crate::identity::Keypair;

    /// Reference test vector for canonical CBOR serialization (Appendix C).
    ///
    /// Uses a fixed keypair and deterministic inputs so other implementations
    /// can reproduce the exact same canonical CBOR bytes and BLAKE3 hash.
    #[test]
    fn canonical_cbor_test_vector() {
        // Fixed secret key (32 bytes of 0x01)
        let secret = [0x01u8; 32];
        let kp = Keypair::from_bytes(&secret);
        let publisher = kp.identity();

        let schema_hash = Hash::blake3(b"mesh:schema:core/capability");
        let routing_key = Hash::blake3(b"mesh:route:compute/inference/text-generation");

        let payload = b"test payload".to_vec();
        let topic = "test-topic".to_string();
        let timestamp: u64 = 1_700_000_000_000_000; // fixed
        let sequence: u64 = 1;
        let ttl: u32 = 3600;
        let routing_keys = vec![routing_key];

        // Print inputs for cross-implementation reference
        println!("=== Test Vector Inputs ===");
        println!("secret_key: {}", hex::encode(secret));
        println!("publisher.algorithm: 0x{:02x}", publisher.algorithm);
        println!(
            "publisher.public_key: {}",
            hex::encode(&publisher.public_key)
        );
        println!("schema_hash.algorithm: 0x{:02x}", schema_hash.algorithm);
        println!("schema_hash.digest: {}", hex::encode(&schema_hash.digest));
        println!("topic: {}", topic);
        println!("payload: {}", hex::encode(&payload));
        println!("timestamp: {}", timestamp);
        println!("sequence: {}", sequence);
        println!("ttl: {}", ttl);
        println!(
            "routing_keys[0].algorithm: 0x{:02x}",
            routing_keys[0].algorithm
        );
        println!(
            "routing_keys[0].digest: {}",
            hex::encode(&routing_keys[0].digest)
        );

        // Compute canonical CBOR
        let cbor_bytes = Descriptor::canonical_cbor_bytes(
            &schema_hash,
            &topic,
            &payload,
            &publisher,
            timestamp,
            sequence,
            ttl,
            &routing_keys,
        );
        let cbor_hex = hex::encode(&cbor_bytes);
        println!("\n=== Canonical CBOR ===");
        println!("cbor_hex: {}", cbor_hex);
        println!("cbor_len: {}", cbor_bytes.len());

        // Compute BLAKE3 hash (the descriptor ID)
        let id = Hash::blake3(&cbor_bytes);
        let id_hex = hex::encode(&id.digest);
        println!("\n=== Descriptor ID ===");
        println!("blake3_hex: {}", id_hex);

        // Also verify via compute_id path
        let id_via_compute = Descriptor::compute_id(
            &schema_hash,
            &topic,
            &payload,
            &publisher,
            timestamp,
            sequence,
            ttl,
            &routing_keys,
        )
        .unwrap();
        assert_eq!(
            id, id_via_compute,
            "canonical_cbor_bytes and compute_id must agree"
        );

        // Now assert the exact values (these become the spec's test vectors).
        // If you change the canonical serialization, these MUST be updated and
        // the PROTOCOL.md appendix MUST be updated to match.
        assert_eq!(
            cbor_hex,
            "a8677061796c6f61644c74657374207061796c6f6164697075626c6973686572820158208a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c6c726f7574696e675f6b6579738182035820235ad34c1ac90b981ab12865b8d4d5d4d18708fc614cec7fd1d01fad82c6b69a6b736368656d615f6861736882035820bd40bb81f07d1e149cc709b581a4c52af445f6a203d7ab32e284a0b3ffcfb3306873657175656e6365016974696d657374616d701b00060a24181e400065746f7069636a746573742d746f7069636374746c190e10",
            "CBOR hex mismatch — canonical serialization has changed!"
        );
        assert_eq!(
            id_hex, "a38b23b474083f46592c4584728addb830f35e98434a0010d3d0b4e63c2f0581",
            "BLAKE3 descriptor ID mismatch — canonical serialization has changed!"
        );

        // Verify full round-trip: create descriptor and validate
        let desc = Descriptor::create(
            &kp,
            schema_hash,
            topic,
            payload,
            timestamp,
            sequence,
            ttl,
            routing_keys,
        )
        .unwrap();
        assert_eq!(desc.id, id_via_compute);
        // Validate at the descriptor's own timestamp (it's in the past but TTL
        // check uses effective_start = min(timestamp, now), so use timestamp + 1s)
        let validate_time = timestamp + 1_000_000;
        assert!(desc.validate(validate_time).is_ok());
    }
}
