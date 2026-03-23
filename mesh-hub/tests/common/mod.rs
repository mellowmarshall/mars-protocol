//! Shared test utilities for mesh-hub integration tests.
#![allow(dead_code)]

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use mesh_core::identity::Keypair;
use mesh_core::Descriptor;
use mesh_core::Hash;
use mesh_hub::storage::redb::RedbStorage;
use mesh_hub::tenant::TenantManager;

/// Current time in microseconds since UNIX epoch.
pub fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Create a test descriptor with sensible defaults.
pub fn make_descriptor(
    kp: &Keypair,
    schema: Hash,
    topic: &str,
    payload: &[u8],
    now: u64,
    seq: u64,
    ttl: u32,
    routing_keys: Vec<Hash>,
) -> Descriptor {
    Descriptor::create(kp, schema, topic.into(), payload.to_vec(), now, seq, ttl, routing_keys)
        .unwrap()
}

/// Open a temporary RedbStorage at the given path.
pub fn open_temp_redb(dir: &Path, name: &str) -> RedbStorage {
    RedbStorage::open(&dir.join(name)).unwrap()
}

/// Open a temporary TenantManager at the given path.
pub fn open_temp_tenant_manager(dir: &Path) -> TenantManager {
    TenantManager::open(&dir.join("tenants.db")).unwrap()
}
