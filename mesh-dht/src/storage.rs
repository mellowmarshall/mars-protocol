//! Descriptor storage — in-memory store keyed by routing key (Section 4).
//!
//! Stores descriptors with deduplication by `publisher + schema_hash + topic`,
//! sequence-based replacement, TTL expiry, per-publisher rate limiting,
//! revocation enforcement, and key-rotation tracking.

use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use mesh_core::message::FilterSet;
use mesh_core::schema::{SCHEMA_HASH_CORE_KEY_ROTATION, SCHEMA_HASH_CORE_REVOCATION};
use mesh_core::{Descriptor, Hash, Identity};

/// Maximum STORE operations per publisher per minute.
const RATE_LIMIT_PER_MINUTE: usize = 10;

/// Rate limit window in microseconds (60 seconds).
const RATE_LIMIT_WINDOW_MICROS: u64 = 60_000_000;

/// Dedup key: identifies a unique descriptor slot per publisher.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DedupKey {
    publisher: Identity,
    schema_hash: Hash,
    topic: String,
}

impl DedupKey {
    fn from_descriptor(desc: &Descriptor) -> Self {
        Self {
            publisher: desc.publisher.clone(),
            schema_hash: desc.schema_hash.clone(),
            topic: desc.topic.clone(),
        }
    }
}

/// Per-publisher rate limiting tracker.
#[derive(Debug, Default)]
struct RateLimiter {
    /// publisher identity → list of STORE timestamps (microseconds).
    timestamps: HashMap<Identity, Vec<u64>>,
}

impl RateLimiter {
    /// Check if a publisher is within rate limits, and record the attempt if so.
    /// Returns `true` if the STORE is allowed.
    fn check_and_record(&mut self, publisher: &Identity, now_micros: u64) -> bool {
        let entries = self.timestamps.entry(publisher.clone()).or_default();

        // Prune old timestamps outside the window
        entries.retain(|&ts| now_micros.saturating_sub(ts) < RATE_LIMIT_WINDOW_MICROS);

        if entries.len() >= RATE_LIMIT_PER_MINUTE {
            return false;
        }

        entries.push(now_micros);
        true
    }
}

/// Trait for pluggable descriptor storage backends.
///
/// The default in-memory implementation is [`DescriptorStore`]. Downstream crates
/// can implement this trait for persistent backends (SQLite, Postgres, etc.).
pub trait DescriptorStorage: Send + Sync {
    /// Store a descriptor after validation.
    fn store_descriptor(&mut self, descriptor: Descriptor) -> Result<(), StoreError>;
    /// Store a descriptor with an explicit timestamp (for testing).
    fn store_descriptor_at(
        &mut self,
        descriptor: Descriptor,
        now_micros: u64,
    ) -> Result<(), StoreError>;
    /// Retrieve descriptors at a routing key, optionally applying filters.
    fn get_descriptors(&self, routing_key: &Hash, filters: Option<&FilterSet>) -> Vec<Descriptor>;
    /// Retrieve descriptors with an explicit timestamp (for testing).
    fn get_descriptors_at(
        &self,
        routing_key: &Hash,
        filters: Option<&FilterSet>,
        now_micros: u64,
    ) -> Vec<Descriptor>;
    /// Remove all expired descriptors from the store.
    fn evict_expired(&mut self);
    /// Remove expired descriptors with an explicit timestamp (for testing).
    fn evict_expired_at(&mut self, now_micros: u64);
    /// Check if we have any descriptors at a routing key.
    fn has_descriptors(&self, routing_key: &Hash) -> bool;
    /// Total number of descriptors stored.
    fn descriptor_count(&self) -> usize;
    /// Number of unique routing keys with stored descriptors.
    fn routing_key_count(&self) -> usize;
}

/// In-memory descriptor storage for the DHT node.
#[derive(Debug)]
pub struct DescriptorStore {
    /// Descriptors indexed by routing key.
    store: HashMap<Hash, Vec<Descriptor>>,
    /// Tracks the highest seen sequence per dedup key for stale detection.
    sequences: HashMap<DedupKey, u64>,
    /// Per-publisher rate limiter.
    rate_limiter: RateLimiter,
    /// Set of revoked descriptor IDs (Section 7 — revocation enforcement).
    revoked: HashSet<Hash>,
    /// Key rotation map: old_identity bytes → (new_identity, rotation_seq).
    /// Tracks identity succession for key-rotation descriptors.
    pub rotations: HashMap<Vec<u8>, (Identity, u64)>,
}

/// Errors from descriptor storage operations.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Descriptor failed validation (Section 2.2).
    #[error("descriptor validation failed: {0}")]
    ValidationFailed(String),
    /// Descriptor sequence is older than what we already have.
    #[error("stale descriptor: sequence {received} < current {current}")]
    StaleDescriptor { received: u64, current: u64 },
    /// Publisher exceeded the per-identity rate limit.
    #[error("rate limited: publisher exceeded {limit} stores per minute")]
    RateLimited { limit: usize },
    /// Revocation rejected: the revoker is not the same publisher as the target.
    #[error("revocation rejected: publisher mismatch")]
    RevocationPublisherMismatch,
    /// Key rotation rejected: stale rotation sequence.
    #[error("stale key rotation: seq {received} <= current {current}")]
    StaleRotation { received: u64, current: u64 },
    /// Key rotation rejected: conflicting rotation for same old identity and seq.
    #[error("key rotation fork detected: conflicting new_identity for same seq")]
    RotationForkDetected,
}

impl DescriptorStore {
    /// Create an empty descriptor store.
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
            sequences: HashMap::new(),
            rate_limiter: RateLimiter::default(),
            revoked: HashSet::new(),
            rotations: HashMap::new(),
        }
    }

    /// Get current time in microseconds since epoch.
    fn now_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }

    /// Store a descriptor after validation.
    ///
    /// Validates the descriptor (Section 2.2 steps 1-7), checks sequence
    /// ordering for dedup, enforces rate limits, and stores under all
    /// routing keys.
    pub fn store_descriptor(&mut self, descriptor: Descriptor) -> Result<(), StoreError> {
        let now = Self::now_micros();
        self.store_descriptor_at(descriptor, now)
    }

    /// Store a descriptor with an explicit timestamp (for testing).
    pub fn store_descriptor_at(
        &mut self,
        descriptor: Descriptor,
        now_micros: u64,
    ) -> Result<(), StoreError> {
        // Validate (Section 2.2 steps 1-7)
        descriptor
            .validate(now_micros)
            .map_err(|e| StoreError::ValidationFailed(e.to_string()))?;

        // Rate limiting (Section 9.3)
        if !self
            .rate_limiter
            .check_and_record(&descriptor.publisher, now_micros)
        {
            return Err(StoreError::RateLimited {
                limit: RATE_LIMIT_PER_MINUTE,
            });
        }

        // Sequence check (Section 2.2 step 8) — dedup key
        let dedup_key = DedupKey::from_descriptor(&descriptor);
        if let Some(&current_seq) = self.sequences.get(&dedup_key)
            && descriptor.sequence < current_seq
        {
            return Err(StoreError::StaleDescriptor {
                received: descriptor.sequence,
                current: current_seq,
            });
        }

        // Update sequence tracker
        self.sequences
            .insert(dedup_key.clone(), descriptor.sequence);

        // Remove older versions with the same dedup key from all routing keys
        for descriptors in self.store.values_mut() {
            descriptors.retain(|d| DedupKey::from_descriptor(d) != dedup_key);
        }

        // Store under each routing key
        let routing_keys = descriptor.routing_keys.clone();
        for key in &routing_keys {
            self.store
                .entry(key.clone())
                .or_default()
                .push(descriptor.clone());
        }

        // Handle revocation descriptors (Section 7)
        if descriptor.schema_hash == *SCHEMA_HASH_CORE_REVOCATION {
            self.process_revocation(&descriptor)?;
        }

        // Handle key-rotation descriptors
        if descriptor.schema_hash == *SCHEMA_HASH_CORE_KEY_ROTATION {
            self.process_key_rotation(&descriptor)?;
        }

        Ok(())
    }

    /// Process a revocation descriptor: verify publisher authority and revoke the target.
    fn process_revocation(&mut self, revocation: &Descriptor) -> Result<(), StoreError> {
        // Parse the CBOR payload to extract target_id
        let value: ciborium::Value = ciborium::from_reader(revocation.payload.as_slice())
            .map_err(|e| StoreError::ValidationFailed(format!("invalid revocation payload: {e}")))?;

        let target_id_bytes = value
            .as_map()
            .and_then(|m| {
                m.iter().find_map(|(k, v)| {
                    if k.as_text() == Some("target_id") {
                        v.as_bytes().cloned()
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| {
                StoreError::ValidationFailed("revocation payload missing target_id".into())
            })?;

        // Reconstruct the target descriptor ID hash from the bytes.
        // The bytes should be [algorithm, ...digest].
        if target_id_bytes.len() < 2 {
            return Err(StoreError::ValidationFailed(
                "target_id too short".into(),
            ));
        }
        let target_id = Hash {
            algorithm: target_id_bytes[0],
            digest: target_id_bytes[1..].to_vec(),
        };

        // Look up the target descriptor to verify publisher match
        let target_desc = self
            .store
            .values()
            .flat_map(|descs| descs.iter())
            .find(|d| d.id == target_id);

        if let Some(target) = target_desc {
            // Verify the revocation publisher matches the target publisher
            if revocation.publisher != target.publisher {
                // Remove the revocation descriptor we just stored (it's invalid)
                for descriptors in self.store.values_mut() {
                    descriptors.retain(|d| d.id != revocation.id);
                }
                return Err(StoreError::RevocationPublisherMismatch);
            }

            // Add target to revoked set and remove from routing key index
            self.revoked.insert(target_id.clone());
            for descriptors in self.store.values_mut() {
                descriptors.retain(|d| d.id != target_id);
            }
            // Clean up empty routing key entries
            self.store.retain(|_, v| !v.is_empty());
        }
        // If target not found, still store the revocation (it may arrive
        // before the target due to replication ordering) and add to revoked set
        // so the target will be filtered when it arrives later.
        self.revoked.insert(target_id);

        Ok(())
    }

    /// Process a key-rotation descriptor: validate and track identity succession.
    fn process_key_rotation(&mut self, rotation: &Descriptor) -> Result<(), StoreError> {
        // Parse the CBOR payload
        let value: ciborium::Value = ciborium::from_reader(rotation.payload.as_slice())
            .map_err(|e| {
                StoreError::ValidationFailed(format!("invalid key-rotation payload: {e}"))
            })?;

        let map = value.as_map().ok_or_else(|| {
            StoreError::ValidationFailed("key-rotation payload is not a map".into())
        })?;

        // Extract fields
        let old_identity_bytes = map
            .iter()
            .find_map(|(k, v)| {
                if k.as_text() == Some("old_identity") {
                    v.as_bytes().cloned()
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                StoreError::ValidationFailed("key-rotation missing old_identity".into())
            })?;

        let new_identity_bytes = map
            .iter()
            .find_map(|(k, v)| {
                if k.as_text() == Some("new_identity") {
                    v.as_bytes().cloned()
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                StoreError::ValidationFailed("key-rotation missing new_identity".into())
            })?;

        let rotation_seq = map
            .iter()
            .find_map(|(k, v)| {
                if k.as_text() == Some("rotation_seq") {
                    v.as_integer()
                        .and_then(|i| u64::try_from(i).ok())
                } else {
                    None
                }
            })
            .ok_or_else(|| {
                StoreError::ValidationFailed("key-rotation missing rotation_seq".into())
            })?;

        // Reconstruct new_identity from bytes: [algorithm, ...public_key]
        if new_identity_bytes.len() < 2 {
            return Err(StoreError::ValidationFailed(
                "new_identity too short".into(),
            ));
        }
        let new_identity = Identity {
            algorithm: new_identity_bytes[0],
            public_key: new_identity_bytes[1..].to_vec(),
        };

        // Check rotation_seq against previous rotation for same old_identity
        if let Some((existing_identity, existing_seq)) = self.rotations.get(&old_identity_bytes) {
            if rotation_seq == *existing_seq && *existing_identity != new_identity {
                // Same seq, different new_identity — fork detection
                for descriptors in self.store.values_mut() {
                    descriptors.retain(|d| d.id != rotation.id);
                }
                return Err(StoreError::RotationForkDetected);
            }
            if rotation_seq < *existing_seq {
                // Stale rotation
                for descriptors in self.store.values_mut() {
                    descriptors.retain(|d| d.id != rotation.id);
                }
                return Err(StoreError::StaleRotation {
                    received: rotation_seq,
                    current: *existing_seq,
                });
            }
            // rotation_seq > existing_seq, or same seq+identity (idempotent) — allow
        }

        // Store the rotation mapping
        self.rotations
            .insert(old_identity_bytes, (new_identity, rotation_seq));

        Ok(())
    }

    /// Retrieve descriptors at a routing key, optionally applying filters.
    ///
    /// Automatically excludes expired descriptors based on effective TTL.
    pub fn get_descriptors(
        &self,
        routing_key: &Hash,
        filters: Option<&FilterSet>,
    ) -> Vec<Descriptor> {
        let now = Self::now_micros();
        self.get_descriptors_at(routing_key, filters, now)
    }

    /// Retrieve descriptors with an explicit timestamp (for testing).
    pub fn get_descriptors_at(
        &self,
        routing_key: &Hash,
        filters: Option<&FilterSet>,
        now_micros: u64,
    ) -> Vec<Descriptor> {
        let Some(descriptors) = self.store.get(routing_key) else {
            return Vec::new();
        };

        descriptors
            .iter()
            .filter(|d| {
                // Filter revoked descriptors
                if self.revoked.contains(&d.id) {
                    return false;
                }
                // Filter expired descriptors using effective TTL
                let effective_start = std::cmp::min(d.timestamp, now_micros);
                let ttl_micros = u64::from(d.ttl) * 1_000_000;
                if effective_start + ttl_micros <= now_micros {
                    return false;
                }
                // Apply user filters
                if let Some(f) = filters {
                    if let Some(ref sh) = f.schema_hash
                        && &d.schema_hash != sh
                    {
                        return false;
                    }
                    if let Some(min_ts) = f.min_timestamp
                        && d.timestamp < min_ts
                    {
                        return false;
                    }
                    if let Some(ref pub_id) = f.publisher
                        && &d.publisher != pub_id
                    {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect()
    }

    /// Remove all expired descriptors from the store.
    pub fn evict_expired(&mut self) {
        let now = Self::now_micros();
        self.evict_expired_at(now);
    }

    /// Remove expired descriptors with an explicit timestamp (for testing).
    pub fn evict_expired_at(&mut self, now_micros: u64) {
        for descriptors in self.store.values_mut() {
            descriptors.retain(|d| {
                let effective_start = std::cmp::min(d.timestamp, now_micros);
                let ttl_micros = u64::from(d.ttl) * 1_000_000;
                effective_start + ttl_micros > now_micros
            });
        }
        // Remove empty routing key entries
        self.store.retain(|_, v| !v.is_empty());
    }

    /// Check if we have any descriptors at a routing key.
    pub fn has_descriptors(&self, routing_key: &Hash) -> bool {
        self.store.get(routing_key).is_some_and(|v| !v.is_empty())
    }

    /// Total number of descriptors stored (across all routing keys, may double-count).
    pub fn descriptor_count(&self) -> usize {
        self.store.values().map(|v| v.len()).sum()
    }

    /// Number of unique routing keys with stored descriptors.
    pub fn routing_key_count(&self) -> usize {
        self.store.len()
    }
}

impl Default for DescriptorStore {
    fn default() -> Self {
        Self::new()
    }
}

impl DescriptorStorage for DescriptorStore {
    fn store_descriptor(&mut self, descriptor: Descriptor) -> Result<(), StoreError> {
        DescriptorStore::store_descriptor(self, descriptor)
    }

    fn store_descriptor_at(
        &mut self,
        descriptor: Descriptor,
        now_micros: u64,
    ) -> Result<(), StoreError> {
        DescriptorStore::store_descriptor_at(self, descriptor, now_micros)
    }

    fn get_descriptors(&self, routing_key: &Hash, filters: Option<&FilterSet>) -> Vec<Descriptor> {
        DescriptorStore::get_descriptors(self, routing_key, filters)
    }

    fn get_descriptors_at(
        &self,
        routing_key: &Hash,
        filters: Option<&FilterSet>,
        now_micros: u64,
    ) -> Vec<Descriptor> {
        DescriptorStore::get_descriptors_at(self, routing_key, filters, now_micros)
    }

    fn evict_expired(&mut self) {
        DescriptorStore::evict_expired(self);
    }

    fn evict_expired_at(&mut self, now_micros: u64) {
        DescriptorStore::evict_expired_at(self, now_micros);
    }

    fn has_descriptors(&self, routing_key: &Hash) -> bool {
        DescriptorStore::has_descriptors(self, routing_key)
    }

    fn descriptor_count(&self) -> usize {
        DescriptorStore::descriptor_count(self)
    }

    fn routing_key_count(&self) -> usize {
        DescriptorStore::routing_key_count(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::hash::schema_hash;
    use mesh_core::identity::Keypair;
    use mesh_core::routing::routing_key;

    fn now_micros() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }

    fn make_descriptor(kp: &Keypair, topic: &str, seq: u64, ttl: u32) -> Descriptor {
        let now = now_micros();
        Descriptor::create(
            kp,
            schema_hash("core/capability"),
            topic.into(),
            b"test payload".to_vec(),
            now,
            seq,
            ttl,
            vec![routing_key("compute/inference/text-generation")],
        )
        .unwrap()
    }

    fn make_descriptor_with_keys(
        kp: &Keypair,
        topic: &str,
        seq: u64,
        routing_keys: Vec<Hash>,
    ) -> Descriptor {
        let now = now_micros();
        Descriptor::create(
            kp,
            schema_hash("core/capability"),
            topic.into(),
            b"test payload".to_vec(),
            now,
            seq,
            3600,
            routing_keys,
        )
        .unwrap()
    }

    #[test]
    fn store_and_retrieve() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let desc = make_descriptor(&kp, "topic", 1, 3600);
        let rk = routing_key("compute/inference/text-generation");

        store.store_descriptor(desc.clone()).unwrap();

        let results = store.get_descriptors(&rk, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, desc.id);
    }

    #[test]
    fn store_multiple_routing_keys() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let keys = vec![
            routing_key("compute"),
            routing_key("compute/inference"),
            routing_key("compute/inference/text-generation"),
        ];
        let desc = make_descriptor_with_keys(&kp, "topic", 1, keys.clone());

        store.store_descriptor(desc).unwrap();

        for key in &keys {
            let results = store.get_descriptors(key, None);
            assert_eq!(results.len(), 1);
        }
        assert_eq!(store.routing_key_count(), 3);
    }

    #[test]
    fn sequence_replacement() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let rk = routing_key("compute/inference/text-generation");

        let desc1 = make_descriptor(&kp, "topic", 1, 3600);
        let desc2 = make_descriptor(&kp, "topic", 2, 3600);

        store.store_descriptor(desc1).unwrap();
        store.store_descriptor(desc2.clone()).unwrap();

        let results = store.get_descriptors(&rk, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sequence, 2);
    }

    #[test]
    fn stale_sequence_rejected() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();

        let desc2 = make_descriptor(&kp, "topic", 2, 3600);
        let desc1 = make_descriptor(&kp, "topic", 1, 3600);

        store.store_descriptor(desc2).unwrap();
        let result = store.store_descriptor(desc1);
        assert!(matches!(result, Err(StoreError::StaleDescriptor { .. })));
    }

    #[test]
    fn different_topics_coexist() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let rk = routing_key("compute/inference/text-generation");

        let desc_a = make_descriptor(&kp, "text-gen", 1, 3600);
        let desc_b = make_descriptor(&kp, "image-gen", 1, 3600);

        store.store_descriptor(desc_a).unwrap();
        store.store_descriptor(desc_b).unwrap();

        let results = store.get_descriptors(&rk, None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn different_publishers_coexist() {
        let mut store = DescriptorStore::new();
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let rk = routing_key("compute/inference/text-generation");

        store
            .store_descriptor(make_descriptor(&kp1, "topic", 1, 3600))
            .unwrap();
        store
            .store_descriptor(make_descriptor(&kp2, "topic", 1, 3600))
            .unwrap();

        let results = store.get_descriptors(&rk, None);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn filter_by_schema() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let rk = routing_key("compute/inference/text-generation");

        store
            .store_descriptor(make_descriptor(&kp, "topic", 1, 3600))
            .unwrap();

        let filter = FilterSet {
            schema_hash: Some(schema_hash("core/capability")),
            ..Default::default()
        };
        let results = store.get_descriptors(&rk, Some(&filter));
        assert_eq!(results.len(), 1);

        let filter_miss = FilterSet {
            schema_hash: Some(schema_hash("core/revocation")),
            ..Default::default()
        };
        let results = store.get_descriptors(&rk, Some(&filter_miss));
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn filter_by_publisher() {
        let mut store = DescriptorStore::new();
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let rk = routing_key("compute/inference/text-generation");

        store
            .store_descriptor(make_descriptor(&kp1, "topic", 1, 3600))
            .unwrap();
        store
            .store_descriptor(make_descriptor(&kp2, "topic", 1, 3600))
            .unwrap();

        let filter = FilterSet {
            publisher: Some(kp1.identity()),
            ..Default::default()
        };
        let results = store.get_descriptors(&rk, Some(&filter));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].publisher, kp1.identity());
    }

    #[test]
    fn filter_by_min_timestamp() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let rk = routing_key("compute/inference/text-generation");

        let desc = make_descriptor(&kp, "topic", 1, 3600);
        let ts = desc.timestamp;
        store.store_descriptor(desc).unwrap();

        // Filter with timestamp before the descriptor — should find it
        let filter = FilterSet {
            min_timestamp: Some(ts - 1_000_000),
            ..Default::default()
        };
        assert_eq!(store.get_descriptors(&rk, Some(&filter)).len(), 1);

        // Filter with timestamp after the descriptor — should miss
        let filter = FilterSet {
            min_timestamp: Some(ts + 1_000_000),
            ..Default::default()
        };
        assert_eq!(store.get_descriptors(&rk, Some(&filter)).len(), 0);
    }

    #[test]
    fn evict_expired() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let rk = routing_key("compute/inference/text-generation");

        // Create a descriptor with 60s TTL
        let desc = make_descriptor(&kp, "topic", 1, 60);
        let ts = desc.timestamp;
        store.store_descriptor(desc).unwrap();

        // Not expired yet
        store.evict_expired_at(ts + 30_000_000); // 30s later
        assert_eq!(store.get_descriptors(&rk, None).len(), 1);

        // Now expired
        store.evict_expired_at(ts + 61_000_000); // 61s later
        assert_eq!(store.get_descriptors(&rk, None).len(), 0);
        assert_eq!(store.routing_key_count(), 0);
    }

    #[test]
    fn rate_limiting() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let now = now_micros();

        // Store 10 descriptors (at the limit)
        for i in 0..10u64 {
            let desc = Descriptor::create(
                &kp,
                schema_hash("core/capability"),
                format!("topic-{i}"),
                b"payload".to_vec(),
                now,
                i + 1,
                3600,
                vec![routing_key("test")],
            )
            .unwrap();
            store.store_descriptor_at(desc, now).unwrap();
        }

        // 11th should be rate limited
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic-overflow".into(),
            b"payload".to_vec(),
            now,
            11,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        let result = store.store_descriptor_at(desc, now);
        assert!(matches!(result, Err(StoreError::RateLimited { .. })));
    }

    #[test]
    fn rate_limit_resets_after_window() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let now = now_micros();

        // Use up the limit
        for i in 0..10u64 {
            let desc = Descriptor::create(
                &kp,
                schema_hash("core/capability"),
                format!("topic-{i}"),
                b"payload".to_vec(),
                now,
                i + 1,
                3600,
                vec![routing_key("test")],
            )
            .unwrap();
            store.store_descriptor_at(desc, now).unwrap();
        }

        // After 60s, should be allowed again
        let later = now + 61_000_000;
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic-after-window".into(),
            b"payload".to_vec(),
            later,
            11,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        store.store_descriptor_at(desc, later).unwrap();
    }

    #[test]
    fn empty_store_returns_empty() {
        let store = DescriptorStore::new();
        let rk = routing_key("nonexistent");
        assert!(store.get_descriptors(&rk, None).is_empty());
        assert!(!store.has_descriptors(&rk));
    }

    #[test]
    fn sequence_replacement_across_routing_keys() {
        // When a newer sequence replaces an older one, it should be removed
        // from ALL routing keys, not just the ones in the new descriptor
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let rk1 = routing_key("compute");
        let rk2 = routing_key("compute/inference");

        let desc1 = make_descriptor_with_keys(&kp, "topic", 1, vec![rk1.clone(), rk2.clone()]);
        store.store_descriptor(desc1).unwrap();
        assert_eq!(store.get_descriptors(&rk1, None).len(), 1);
        assert_eq!(store.get_descriptors(&rk2, None).len(), 1);

        // New version with same dedup key replaces across all routing keys
        let desc2 = make_descriptor_with_keys(&kp, "topic", 2, vec![rk1.clone(), rk2.clone()]);
        store.store_descriptor(desc2).unwrap();
        assert_eq!(store.get_descriptors(&rk1, None).len(), 1);
        assert_eq!(store.get_descriptors(&rk2, None).len(), 1);
        // Verify it's the new one
        assert_eq!(store.get_descriptors(&rk1, None)[0].sequence, 2);
        assert_eq!(store.get_descriptors(&rk2, None)[0].sequence, 2);
    }

    #[test]
    fn expired_descriptors_filtered_on_read() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let rk = routing_key("compute/inference/text-generation");
        let now = now_micros();

        // Create descriptor with 60s TTL
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            60,
            vec![rk.clone()],
        )
        .unwrap();
        store.store_descriptor_at(desc, now).unwrap();

        // Should be visible now
        assert_eq!(
            store.get_descriptors_at(&rk, None, now + 30_000_000).len(),
            1
        );

        // Should be filtered out when expired (without calling evict_expired)
        assert_eq!(
            store.get_descriptors_at(&rk, None, now + 61_000_000).len(),
            0
        );
    }

    #[test]
    fn equal_sequence_accepted() {
        // Spec says >= not >. Equal sequence should be accepted (idempotent republish)
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();

        let desc1 = make_descriptor(&kp, "topic", 5, 3600);
        store.store_descriptor(desc1).unwrap();

        // Same sequence should succeed (not be rejected as stale)
        let desc2 = make_descriptor(&kp, "topic", 5, 3600);
        assert!(store.store_descriptor(desc2).is_ok());
    }

    #[test]
    fn rate_limit_per_publisher_independent() {
        // Rate limits are per-publisher, not global
        let mut store = DescriptorStore::new();
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let now = now_micros();

        // Fill up kp1's rate limit
        for i in 0..10u64 {
            let desc = Descriptor::create(
                &kp1,
                schema_hash("core/capability"),
                format!("topic-{i}"),
                b"payload".to_vec(),
                now,
                i + 1,
                3600,
                vec![routing_key("test")],
            )
            .unwrap();
            store.store_descriptor_at(desc, now).unwrap();
        }

        // kp2 should still be able to publish
        let desc = Descriptor::create(
            &kp2,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![routing_key("test")],
        )
        .unwrap();
        assert!(store.store_descriptor_at(desc, now).is_ok());
    }

    // ── Revocation Tests (B2) ──

    /// Helper: build a CBOR revocation payload with target_id.
    fn make_revocation_payload(target_id: &Hash) -> Vec<u8> {
        use ciborium::Value;
        let mut id_bytes = vec![target_id.algorithm];
        id_bytes.extend_from_slice(&target_id.digest);
        let map = Value::Map(vec![(
            Value::Text("target_id".into()),
            Value::Bytes(id_bytes),
        )]);
        let mut buf = Vec::new();
        ciborium::into_writer(&map, &mut buf).unwrap();
        buf
    }

    #[test]
    fn revocation_removes_target() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let rk = routing_key("compute");
        let now = now_micros();

        // Store a normal descriptor
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        let target_id = desc.id.clone();
        store.store_descriptor_at(desc, now).unwrap();
        assert_eq!(store.get_descriptors_at(&rk, None, now).len(), 1);

        // Store a revocation for it (same publisher)
        let revocation_payload = make_revocation_payload(&target_id);
        let revocation = Descriptor::create(
            &kp,
            SCHEMA_HASH_CORE_REVOCATION.clone(),
            "revoke-topic".into(),
            revocation_payload,
            now,
            2,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        store.store_descriptor_at(revocation.clone(), now).unwrap();

        // The revoked descriptor should not be returned
        let results = store.get_descriptors_at(&rk, None, now);
        assert!(
            !results.iter().any(|d| d.id == target_id),
            "revoked descriptor should not be returned"
        );

        // The revocation descriptor itself should still be retrievable
        assert!(
            results.iter().any(|d| d.id == revocation.id),
            "revocation descriptor should be retrievable"
        );
    }

    #[test]
    fn revocation_by_wrong_publisher_rejected() {
        let mut store = DescriptorStore::new();
        let kp_owner = Keypair::generate();
        let kp_attacker = Keypair::generate();
        let rk = routing_key("compute");
        let now = now_micros();

        // Store a normal descriptor from kp_owner
        let desc = Descriptor::create(
            &kp_owner,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        let target_id = desc.id.clone();
        store.store_descriptor_at(desc, now).unwrap();

        // Attacker tries to revoke it
        let revocation_payload = make_revocation_payload(&target_id);
        let revocation = Descriptor::create(
            &kp_attacker,
            SCHEMA_HASH_CORE_REVOCATION.clone(),
            "revoke-topic".into(),
            revocation_payload,
            now,
            1,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        let result = store.store_descriptor_at(revocation, now);
        assert!(
            matches!(result, Err(StoreError::RevocationPublisherMismatch)),
            "revocation by wrong publisher should be rejected"
        );

        // Original descriptor should still be available
        let results = store.get_descriptors_at(&rk, None, now);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, target_id);
    }

    #[test]
    fn revocation_descriptor_itself_stored() {
        let mut store = DescriptorStore::new();
        let kp = Keypair::generate();
        let rk = routing_key("compute");
        let now = now_micros();

        // Store target
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        let target_id = desc.id.clone();
        store.store_descriptor_at(desc, now).unwrap();

        // Revoke
        let revocation_payload = make_revocation_payload(&target_id);
        let revocation = Descriptor::create(
            &kp,
            SCHEMA_HASH_CORE_REVOCATION.clone(),
            "revoke-topic".into(),
            revocation_payload,
            now,
            2,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        let rev_id = revocation.id.clone();
        store.store_descriptor_at(revocation, now).unwrap();

        // The revocation descriptor itself should be stored and retrievable
        let results = store.get_descriptors_at(&rk, None, now);
        assert!(results.iter().any(|d| d.id == rev_id));
    }

    // ── Key Rotation Tests (B3) ──

    /// Helper: build a CBOR key-rotation payload.
    fn make_key_rotation_payload(
        old_identity: &Identity,
        new_identity: &Identity,
        rotation_seq: u64,
    ) -> Vec<u8> {
        use ciborium::Value;
        let mut old_bytes = vec![old_identity.algorithm];
        old_bytes.extend_from_slice(&old_identity.public_key);
        let mut new_bytes = vec![new_identity.algorithm];
        new_bytes.extend_from_slice(&new_identity.public_key);
        let map = Value::Map(vec![
            (
                Value::Text("old_identity".into()),
                Value::Bytes(old_bytes),
            ),
            (
                Value::Text("new_identity".into()),
                Value::Bytes(new_bytes),
            ),
            (
                Value::Text("rotation_seq".into()),
                Value::Integer(rotation_seq.into()),
            ),
        ]);
        let mut buf = Vec::new();
        ciborium::into_writer(&map, &mut buf).unwrap();
        buf
    }

    fn identity_bytes_for(id: &Identity) -> Vec<u8> {
        let mut bytes = vec![id.algorithm];
        bytes.extend_from_slice(&id.public_key);
        bytes
    }

    #[test]
    fn key_rotation_updates_map() {
        let mut store = DescriptorStore::new();
        let old_kp = Keypair::generate();
        let new_kp = Keypair::generate();
        let rk = routing_key("compute");
        let now = now_micros();

        let payload =
            make_key_rotation_payload(&old_kp.identity(), &new_kp.identity(), 1);
        let rotation_desc = Descriptor::create(
            &old_kp,
            SCHEMA_HASH_CORE_KEY_ROTATION.clone(),
            "rotation".into(),
            payload,
            now,
            1,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        store.store_descriptor_at(rotation_desc, now).unwrap();

        // Verify rotations map was updated
        let old_bytes = identity_bytes_for(&old_kp.identity());
        let (new_id, seq) = store.rotations.get(&old_bytes).expect("rotation should be tracked");
        assert_eq!(*new_id, new_kp.identity());
        assert_eq!(*seq, 1);
    }

    #[test]
    fn key_rotation_stale_seq_rejected() {
        let mut store = DescriptorStore::new();
        let old_kp = Keypair::generate();
        let new_kp1 = Keypair::generate();
        let new_kp2 = Keypair::generate();
        let rk = routing_key("compute");
        let now = now_micros();

        // First rotation at seq 5
        let payload1 =
            make_key_rotation_payload(&old_kp.identity(), &new_kp1.identity(), 5);
        let rot1 = Descriptor::create(
            &old_kp,
            SCHEMA_HASH_CORE_KEY_ROTATION.clone(),
            "rotation-1".into(),
            payload1,
            now,
            1,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        store.store_descriptor_at(rot1, now).unwrap();

        // Try stale rotation at seq 3
        let payload2 =
            make_key_rotation_payload(&old_kp.identity(), &new_kp2.identity(), 3);
        let rot2 = Descriptor::create(
            &old_kp,
            SCHEMA_HASH_CORE_KEY_ROTATION.clone(),
            "rotation-2".into(),
            payload2,
            now,
            2,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        let result = store.store_descriptor_at(rot2, now);
        assert!(
            matches!(result, Err(StoreError::StaleRotation { .. })),
            "stale rotation_seq should be rejected"
        );

        // Original rotation should still be in place
        let old_bytes = identity_bytes_for(&old_kp.identity());
        let (new_id, seq) = store.rotations.get(&old_bytes).unwrap();
        assert_eq!(*new_id, new_kp1.identity());
        assert_eq!(*seq, 5);
    }

    #[test]
    fn key_rotation_fork_detected() {
        let mut store = DescriptorStore::new();
        let old_kp = Keypair::generate();
        let new_kp1 = Keypair::generate();
        let new_kp2 = Keypair::generate();
        let rk = routing_key("compute");
        let now = now_micros();

        // First rotation at seq 1
        let payload1 =
            make_key_rotation_payload(&old_kp.identity(), &new_kp1.identity(), 1);
        let rot1 = Descriptor::create(
            &old_kp,
            SCHEMA_HASH_CORE_KEY_ROTATION.clone(),
            "rotation-1".into(),
            payload1,
            now,
            1,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        store.store_descriptor_at(rot1, now).unwrap();

        // Same seq but different new_identity — fork!
        let payload2 =
            make_key_rotation_payload(&old_kp.identity(), &new_kp2.identity(), 1);
        let rot2 = Descriptor::create(
            &old_kp,
            SCHEMA_HASH_CORE_KEY_ROTATION.clone(),
            "rotation-2".into(),
            payload2,
            now,
            2,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        let result = store.store_descriptor_at(rot2, now);
        assert!(
            matches!(result, Err(StoreError::RotationForkDetected)),
            "fork should be detected when same seq but different new_identity"
        );
    }
}
