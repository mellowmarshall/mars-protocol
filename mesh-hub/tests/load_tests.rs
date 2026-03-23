//! Phase 4 hardening: storage load + eviction pressure tests for RedbStorage.

use std::time::{SystemTime, UNIX_EPOCH};

use mesh_core::hash::schema_hash;
use mesh_core::identity::Keypair;
use mesh_core::routing::routing_key;
use mesh_core::Descriptor;
use mesh_dht::storage::DescriptorStorage;
use mesh_hub::storage::redb::RedbStorage;

fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Store 1000 descriptors with varied routing keys, publishers, schemas
/// and verify all are retrievable and counted correctly.
#[test]
fn test_high_volume_store_and_retrieve() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = RedbStorage::open(&dir.path().join("load.redb")).unwrap();
    let now = now_micros();

    // Use 100 different publishers (10 descriptors each) to avoid rate limits.
    // Use 10 different routing keys and 5 schema types for variety.
    let publishers: Vec<Keypair> = (0..100).map(|_| Keypair::generate()).collect();
    let routing_keys: Vec<_> = (0..10)
        .map(|i| routing_key(&format!("compute/service-{i}")))
        .collect();
    let schemas: Vec<_> = (0..5)
        .map(|i| schema_hash(&format!("core/type-{i}")))
        .collect();

    let mut stored_count = 0;
    for (pub_idx, kp) in publishers.iter().enumerate() {
        for desc_idx in 0..10 {
            let rk_idx = (pub_idx * 10 + desc_idx) % routing_keys.len();
            let schema_idx = (pub_idx * 10 + desc_idx) % schemas.len();
            let desc = Descriptor::create(
                kp,
                schemas[schema_idx].clone(),
                format!("topic-{desc_idx}"),
                format!("payload-{pub_idx}-{desc_idx}").into_bytes(),
                now,
                (desc_idx + 1) as u64,
                3600,
                vec![routing_keys[rk_idx].clone()],
            )
            .unwrap();
            store.store_descriptor_at(desc, now).unwrap();
            stored_count += 1;
        }
    }

    assert_eq!(stored_count, 1000);
    assert_eq!(store.descriptor_count(), 1000);

    // Verify each routing key returns the correct number of descriptors.
    // Each routing key should have 100 descriptors (1000 / 10 routing keys).
    for rk in &routing_keys {
        let results = store.get_descriptors_at(rk, None, now);
        assert_eq!(
            results.len(),
            100,
            "each routing key should have 100 descriptors"
        );
    }
}

/// Store descriptors with short and long TTLs, then evict at a time
/// that only expires the short-lived ones.
#[test]
fn test_eviction_under_expiry_pressure() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = RedbStorage::open(&dir.path().join("eviction.redb")).unwrap();
    let now = now_micros();

    // Use many publishers to avoid per-publisher rate limits (10 per publisher).
    let short_ttl_publishers: Vec<Keypair> = (0..10).map(|_| Keypair::generate()).collect();
    let long_ttl_publishers: Vec<Keypair> = (0..10).map(|_| Keypair::generate()).collect();

    let rk = routing_key("compute/eviction-test");

    // Store 100 descriptors with TTL=60s (minimum)
    for (pub_idx, kp) in short_ttl_publishers.iter().enumerate() {
        for i in 0..10 {
            let desc = Descriptor::create(
                kp,
                schema_hash("core/capability"),
                format!("short-{i}"),
                format!("short-payload-{pub_idx}-{i}").into_bytes(),
                now,
                (i + 1) as u64,
                60, // 60s TTL
                vec![rk.clone()],
            )
            .unwrap();
            store.store_descriptor_at(desc, now).unwrap();
        }
    }

    // Store 100 more with TTL=3600s
    for (pub_idx, kp) in long_ttl_publishers.iter().enumerate() {
        for i in 0..10 {
            let desc = Descriptor::create(
                kp,
                schema_hash("core/capability"),
                format!("long-{i}"),
                format!("long-payload-{pub_idx}-{i}").into_bytes(),
                now,
                (i + 1) as u64,
                3600, // 1 hour TTL
                vec![rk.clone()],
            )
            .unwrap();
            store.store_descriptor_at(desc, now).unwrap();
        }
    }

    assert_eq!(store.descriptor_count(), 200);

    // Evict at now + 120s — short TTL (60s) should be expired, long TTL (3600s) should remain
    let evict_time = now + 120 * 1_000_000; // 120 seconds in microseconds
    store.evict_expired_at(evict_time);

    assert_eq!(
        store.descriptor_count(),
        100,
        "only the 100 long-TTL descriptors should remain"
    );

    // Verify only long-TTL descriptors are returned
    let results = store.get_descriptors_at(&rk, None, evict_time);
    assert_eq!(results.len(), 100);
    for desc in &results {
        assert_eq!(desc.ttl, 3600, "all remaining should have long TTL");
    }
}

/// For 50 publishers, each stores 10 descriptor updates (sequence 1-10).
/// Only the latest (sequence 10) should remain per (publisher, schema, topic).
#[test]
fn test_sequence_replacement_at_scale() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = RedbStorage::open(&dir.path().join("sequence.redb")).unwrap();
    let now = now_micros();

    let publishers: Vec<Keypair> = (0..50).map(|_| Keypair::generate()).collect();
    let rk = routing_key("compute/sequence-test");

    for kp in &publishers {
        for seq in 1..=10u64 {
            let desc = Descriptor::create(
                kp,
                schema_hash("core/capability"),
                "topic".into(),
                format!("payload-seq-{seq}").into_bytes(),
                now,
                seq,
                3600,
                vec![rk.clone()],
            )
            .unwrap();
            store.store_descriptor_at(desc, now).unwrap();
        }
    }

    // Each publisher should only have 1 descriptor (the latest sequence)
    assert_eq!(
        store.descriptor_count(),
        50,
        "should have exactly 50 descriptors (one per publisher)"
    );

    let results = store.get_descriptors_at(&rk, None, now);
    assert_eq!(results.len(), 50);
    for desc in &results {
        assert_eq!(
            desc.sequence, 10,
            "only the latest sequence should remain"
        );
    }
}

/// Store descriptors across 100 different routing keys and verify
/// each routing key returns only its own descriptors.
#[test]
fn test_concurrent_routing_key_queries() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = RedbStorage::open(&dir.path().join("routing.redb")).unwrap();
    let now = now_micros();

    let routing_keys: Vec<_> = (0..100)
        .map(|i| routing_key(&format!("compute/rk-{i}")))
        .collect();

    // Use 100 publishers (one per routing key) to stay within rate limits
    let publishers: Vec<Keypair> = (0..100).map(|_| Keypair::generate()).collect();

    // Store one descriptor per routing key
    for (i, (rk, kp)) in routing_keys.iter().zip(publishers.iter()).enumerate() {
        let desc = Descriptor::create(
            kp,
            schema_hash("core/capability"),
            format!("topic-{i}"),
            format!("payload-{i}").into_bytes(),
            now,
            1,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        store.store_descriptor_at(desc, now).unwrap();
    }

    assert_eq!(store.descriptor_count(), 100);

    // Query each routing key and verify exactly 1 descriptor returned
    for (i, rk) in routing_keys.iter().enumerate() {
        let results = store.get_descriptors_at(rk, None, now);
        assert_eq!(
            results.len(),
            1,
            "routing key {i} should have exactly 1 descriptor"
        );
        assert_eq!(results[0].topic, format!("topic-{i}"));
    }
}
