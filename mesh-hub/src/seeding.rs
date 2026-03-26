//! Built-in hub seeding — pre-populates storage with a seed file.
//!
//! The hub reads a JSON seed file on startup and periodically re-seeds
//! to keep descriptors alive past TTL. No external gateway or cron needed.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use mesh_core::hash::schema_hash;
use mesh_core::identity::Keypair;
use mesh_core::routing::hierarchical_routing_keys;
use mesh_core::Descriptor;
use mesh_dht::DescriptorStorage;

use crate::HubDhtNode;

/// A service entry from the seed file.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct SeedEntry {
    pub r#type: String,
    pub endpoint: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

/// Load seed entries from a JSON file.
///
/// Expected format: `[{"type": "...", "endpoint": "...", "params": {...}}, ...]`
pub fn load_seed_file(path: &Path) -> Result<Vec<SeedEntry>, Box<dyn std::error::Error>> {
    let content = std::fs::read_to_string(path)?;
    let entries: Vec<SeedEntry> = serde_json::from_str(&content)?;
    tracing::info!(count = entries.len(), path = %path.display(), "loaded seed file");
    Ok(entries)
}

/// Seed the hub's storage directly with descriptors built from seed entries.
///
/// Uses the hub's own keypair to sign descriptors. Bypasses policy checks
/// and rate limits — this is the hub's own data, not an external submission.
pub fn seed_now(
    dht_node: &Arc<Mutex<HubDhtNode>>,
    keypair: &Keypair,
    entries: &[SeedEntry],
) -> (usize, usize) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64;

    let mut ok = 0;
    let mut fail = 0;

    let mut node = dht_node.lock().unwrap();

    // Bypass rate limits for internal seeding
    node.store.inner_mut().skip_rate_limit = true;

    for entry in entries {
        // Build payload as JSON bytes
        let payload = if let Some(ref params) = entry.params {
            let mut map = serde_json::Map::new();
            map.insert("type".into(), serde_json::Value::String(entry.r#type.clone()));
            map.insert(
                "endpoint".into(),
                serde_json::Value::String(entry.endpoint.clone()),
            );
            if let serde_json::Value::Object(p) = params {
                for (k, v) in p {
                    map.insert(k.clone(), v.clone());
                }
            }
            serde_json::to_vec(&map).unwrap_or_default()
        } else {
            serde_json::to_vec(&serde_json::json!({
                "type": entry.r#type,
                "endpoint": entry.endpoint,
            }))
            .unwrap_or_default()
        };

        let routing_keys = hierarchical_routing_keys(&entry.r#type);

        match Descriptor::create(
            keypair,
            schema_hash("core/capability"),
            entry.r#type.clone(),
            payload,
            now,
            1,
            3600, // 1 hour TTL
            routing_keys,
        ) {
            Ok(descriptor) => match node.store.store_descriptor(descriptor) {
                Ok(_) => ok += 1,
                Err(e) => {
                    tracing::warn!(error = %e, service = %entry.r#type, "seed store failed");
                    fail += 1;
                }
            },
            Err(e) => {
                tracing::warn!(error = %e, service = %entry.r#type, "seed descriptor creation failed");
                fail += 1;
            }
        }
    }

    // Restore rate limiting for external requests
    node.store.inner_mut().skip_rate_limit = false;

    tracing::info!(ok, fail, total = entries.len(), "seeding complete");
    (ok, fail)
}

/// Spawn a background task that re-seeds storage periodically.
pub fn spawn_seeder(
    dht_node: Arc<Mutex<HubDhtNode>>,
    keypair: Keypair,
    entries: Vec<SeedEntry>,
    interval: Duration,
) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;
            let (ok, fail) = seed_now(&dht_node, &keypair, &entries);
            tracing::info!(ok, fail, next_in_secs = interval.as_secs(), "re-seed cycle");
        }
    });
}
