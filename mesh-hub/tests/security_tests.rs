//! Phase 4 hardening: security verification tests.

mod common;

use mesh_core::identity::Keypair;
use mesh_core::message::{NodeAddr, NodeInfo};
use mesh_core::routing::routing_key;
use mesh_core::schema::{SCHEMA_HASH_CORE_CAPABILITY, SCHEMA_HASH_CORE_REVOCATION};
use mesh_core::Hash;
use mesh_dht::routing::{AddNodeResult, K, RoutingTable};
use mesh_dht::storage::DescriptorStore;
use mesh_dht::verify_sender_binding;
use mesh_hub::network::validate_outbound_addr;

use common::{make_descriptor, now_micros, open_temp_tenant_manager};

// ── Test 1: Sender binding verification ──

#[test]
fn test_sender_binding_rejects_mismatch() {
    let kp_a = Keypair::generate();
    let kp_b = Keypair::generate();
    let id_a = kp_a.identity();
    let id_b = kp_b.identity();

    // Matching identities → Ok
    assert!(verify_sender_binding(&id_a, &Some(id_a.clone())).is_ok());

    // Mismatched identities → Err
    let err = verify_sender_binding(&id_a, &Some(id_b)).unwrap_err();
    assert!(
        err.contains("sender-TLS binding failed"),
        "should mention binding failure: {err}"
    );

    // None peer identity → Err
    let err = verify_sender_binding(&id_a, &None).unwrap_err();
    assert!(
        err.contains("no peer identity"),
        "should mention missing peer identity: {err}"
    );
}

// ── Test 2: Address validation blocks private ranges ──

#[test]
fn test_address_validation_blocks_private() {
    // RFC1918: 10.0.0.0/8
    assert!(validate_outbound_addr("10.0.0.1:4433", &[]).is_err());
    assert!(validate_outbound_addr("10.255.255.255:4433", &[]).is_err());

    // RFC1918: 172.16.0.0/12
    assert!(validate_outbound_addr("172.16.0.1:4433", &[]).is_err());
    assert!(validate_outbound_addr("172.31.255.255:4433", &[]).is_err());

    // RFC1918: 192.168.0.0/16
    assert!(validate_outbound_addr("192.168.0.1:4433", &[]).is_err());
    assert!(validate_outbound_addr("192.168.255.255:4433", &[]).is_err());

    // Loopback
    assert!(validate_outbound_addr("127.0.0.1:4433", &[]).is_err());

    // Link-local
    assert!(validate_outbound_addr("169.254.1.1:4433", &[]).is_err());

    // CGN
    assert!(validate_outbound_addr("100.64.0.1:4433", &[]).is_err());

    // IPv6 loopback
    assert!(validate_outbound_addr("[::1]:4433", &[]).is_err());

    // IPv6 ULA
    assert!(validate_outbound_addr("[fd00::1]:4433", &[]).is_err());

    // Unspecified
    assert!(validate_outbound_addr("0.0.0.0:4433", &[]).is_err());

    // Public IPs pass
    assert!(validate_outbound_addr("8.8.8.8:443", &[]).is_ok());
    assert!(validate_outbound_addr("1.1.1.1:443", &[]).is_ok());
    assert!(validate_outbound_addr("203.0.113.1:4433", &[]).is_ok());

    // Allowlist overrides
    let allowlist = vec!["127.0.0.1:4433".to_string()];
    assert!(validate_outbound_addr("127.0.0.1:4433", &allowlist).is_ok());

    // Allowlist is exact-match: different port should still be blocked
    assert!(validate_outbound_addr("127.0.0.1:9999", &allowlist).is_err());
}

// ── Test 3: Revocation enforcement ──

/// Helper to build a revocation CBOR payload containing the target descriptor ID.
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
fn test_revocation_enforcement() {
    let mut store = DescriptorStore::new();
    let kp = Keypair::generate();
    let rk = routing_key("compute/revocation-test");
    let now = now_micros();

    let desc = make_descriptor(
        &kp,
        SCHEMA_HASH_CORE_CAPABILITY.clone(),
        "topic",
        b"payload",
        now,
        1,
        3600,
        vec![rk.clone()],
    );
    let target_id = desc.id.clone();
    store.store_descriptor_at(desc, now).unwrap();

    assert_eq!(store.get_descriptors_at(&rk, None, now).len(), 1);

    // Store a revocation descriptor (same publisher)
    let revocation_payload = make_revocation_payload(&target_id);
    let revocation = make_descriptor(
        &kp,
        SCHEMA_HASH_CORE_REVOCATION.clone(),
        "revoke-topic",
        &revocation_payload,
        now,
        2,
        3600,
        vec![rk.clone()],
    );
    store.store_descriptor_at(revocation, now).unwrap();

    // The original descriptor should no longer be returned
    let results = store.get_descriptors_at(&rk, None, now);
    for desc in &results {
        assert_ne!(
            desc.id, target_id,
            "revoked descriptor should not appear in queries"
        );
    }
}

// ── Test 4: Routing table Sybil resistance ──

fn make_node_info_with_time(kp: &Keypair, last_seen: u64) -> NodeInfo {
    NodeInfo {
        identity: kp.identity(),
        addr: NodeAddr {
            protocol: "quic".into(),
            address: "127.0.0.1:4433".into(),
        },
        last_seen,
    }
}

const KEYGEN_SAFETY_CAP: usize = 100_000;

#[test]
fn test_routing_table_sybil_resistance() {
    use mesh_dht::distance::{bucket_index, xor_distance};

    let local = Keypair::generate();
    let local_id = local.identity().node_id();
    let mut table = RoutingTable::new(local_id.clone());

    // Generate K+1 nodes that land in the same bucket (capped to prevent CI hangs)
    let mut nodes_for_bucket: Vec<Keypair> = Vec::new();
    let mut target_bucket = None;

    for _ in 0..KEYGEN_SAFETY_CAP {
        let kp = Keypair::generate();
        let node_id = kp.identity().node_id();
        let dist = xor_distance(&local_id, &node_id);
        if let Some(idx) = bucket_index(&dist) {
            if target_bucket.is_none() || target_bucket == Some(idx) {
                target_bucket = Some(idx);
                nodes_for_bucket.push(kp);
                if nodes_for_bucket.len() > K {
                    break;
                }
            }
        }
    }
    assert!(
        nodes_for_bucket.len() > K,
        "failed to generate K+1 nodes in same bucket within {KEYGEN_SAFETY_CAP} attempts"
    );

    let bucket_idx = target_bucket.unwrap();

    // Add the first K nodes — should all succeed
    for (i, kp) in nodes_for_bucket.iter().take(K).enumerate() {
        let result = table.add_node(make_node_info_with_time(kp, i as u64));
        assert!(
            matches!(result, AddNodeResult::Added),
            "node {i} should be added"
        );
    }
    assert_eq!(table.bucket(bucket_idx).entries.len(), K);

    // The (K+1)th node should trigger BucketFull
    let overflow_kp = &nodes_for_bucket[K];
    let result = table.add_node(make_node_info_with_time(overflow_kp, K as u64));
    let (lrs_id, candidate_info) = match result {
        AddNodeResult::BucketFull { lrs, candidate } => {
            assert_eq!(lrs.identity, nodes_for_bucket[0].identity());
            assert_eq!(candidate.identity, overflow_kp.identity());
            (lrs.identity.node_id(), candidate)
        }
        _ => panic!("expected BucketFull, got {:?}", result),
    };

    assert_eq!(table.bucket(bucket_idx).entries.len(), K);

    // Resolve as LRS responded — candidate discarded
    table.resolve_challenge(&lrs_id, candidate_info.clone(), true);
    assert_eq!(
        table.bucket(bucket_idx).entries.len(),
        K,
        "bucket should still have K entries after LRS responded"
    );
    assert!(
        !table
            .bucket(bucket_idx)
            .entries
            .iter()
            .any(|e| e.identity == overflow_kp.identity()),
        "overflow node should not be admitted when LRS responded"
    );

    // Trigger BucketFull again with a new candidate
    let new_kp = {
        let mut found = None;
        for _ in 0..KEYGEN_SAFETY_CAP {
            let kp = Keypair::generate();
            let node_id = kp.identity().node_id();
            let dist = xor_distance(&local_id, &node_id);
            if bucket_index(&dist) == Some(bucket_idx) {
                found = Some(kp);
                break;
            }
        }
        found.unwrap_or_else(|| {
            panic!("failed to generate node for bucket {bucket_idx} within {KEYGEN_SAFETY_CAP} attempts")
        })
    };
    let result = table.add_node(make_node_info_with_time(&new_kp, (K + 1) as u64));
    let (lrs_id2, candidate_info2) = match result {
        AddNodeResult::BucketFull { lrs, candidate } => (lrs.identity.node_id(), candidate),
        _ => panic!("expected BucketFull"),
    };

    // Resolve as LRS dead — candidate admitted
    table.resolve_challenge(&lrs_id2, candidate_info2, false);
    assert_eq!(
        table.bucket(bucket_idx).entries.len(),
        K,
        "bucket should still have K entries after eviction"
    );
    assert!(
        table
            .bucket(bucket_idx)
            .entries
            .iter()
            .any(|e| e.identity == new_kp.identity()),
        "new candidate should be admitted when LRS is dead"
    );
}

// ── Test 5: DID-Auth challenge lifecycle ──

#[test]
fn test_did_auth_challenge_lifecycle() {
    let dir = tempfile::tempdir().unwrap();
    let tm = open_temp_tenant_manager(dir.path());
    let kp = Keypair::generate();
    let identity = kp.identity();

    let tenant = tm.create_tenant("auth-test", "free").unwrap();

    // Create challenge
    let challenge = tm
        .create_challenge(&tenant.id, &identity.did(), "register_identity")
        .unwrap();

    assert!(
        !challenge.is_expired(challenge.issued_at),
        "challenge should not be expired at issued time"
    );
    assert!(
        !challenge.is_expired(challenge.issued_at + 1_000_000),
        "challenge should not be expired 1s after issued"
    );

    // Sign with correct key → verify succeeds
    let signable = challenge.to_signable_bytes();
    let signature = kp.sign(&signable);
    assert!(
        challenge.verify(&identity, &signature).is_ok(),
        "valid signature should verify"
    );

    // Sign with wrong key → verify fails
    let wrong_kp = Keypair::generate();
    let wrong_sig = wrong_kp.sign(&signable);
    assert!(
        challenge.verify(&identity, &wrong_sig).is_err(),
        "wrong signature should fail"
    );

    // Consume the challenge
    assert!(
        tm.consume_challenge(&challenge.id).is_ok(),
        "first consume should succeed"
    );

    // Replay → fails
    assert!(
        tm.consume_challenge(&challenge.id).is_err(),
        "second consume should fail (already consumed)"
    );

    // Expiry check
    let challenge2 = tm
        .create_challenge(&tenant.id, &identity.did(), "register_identity")
        .unwrap();
    assert!(
        challenge2.is_expired(challenge2.expiry),
        "challenge should be expired at expiry time"
    );
    assert!(
        challenge2.is_expired(challenge2.expiry + 1_000_000),
        "challenge should be expired after expiry time"
    );
}
