//! Descriptor storage — in-memory store keyed by routing key (Section 4).
//!
//! Stores descriptors with deduplication by `publisher + schema_hash + topic`,
//! sequence-based replacement, TTL expiry, and per-publisher rate limiting.

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use mesh_core::message::FilterSet;
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

/// In-memory descriptor storage for the DHT node.
#[derive(Debug)]
pub struct DescriptorStore {
    /// Descriptors indexed by routing key.
    store: HashMap<Hash, Vec<Descriptor>>,
    /// Tracks the highest seen sequence per dedup key for stale detection.
    sequences: HashMap<DedupKey, u64>,
    /// Per-publisher rate limiter.
    rate_limiter: RateLimiter,
}

/// Errors from storage operations.
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
}

impl DescriptorStore {
    /// Create an empty descriptor store.
    pub fn new() -> Self {
        Self {
            store: HashMap::new(),
            sequences: HashMap::new(),
            rate_limiter: RateLimiter::default(),
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
}
