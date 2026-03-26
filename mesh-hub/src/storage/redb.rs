//! Disk-backed descriptor storage using redb (L2 warm store).

use std::collections::HashMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use mesh_core::message::{from_cbor, to_cbor, FilterSet};
use mesh_core::{Descriptor, Hash, Identity};
use mesh_dht::storage::{DescriptorStorage, StoreError};
use redb::{Database, MultimapTableDefinition, ReadableTable, ReadableTableMetadata, TableDefinition};

const DESCRIPTORS: TableDefinition<&[u8], &[u8]> = TableDefinition::new("descriptors");
const BY_ROUTING_KEY: MultimapTableDefinition<&[u8], &[u8]> =
    MultimapTableDefinition::new("by_routing_key");
const DEDUP_INDEX: TableDefinition<&[u8], &[u8]> = TableDefinition::new("dedup_index");
const SEQUENCES: TableDefinition<&[u8], u64> = TableDefinition::new("sequences");

const RATE_LIMIT_PER_MINUTE: usize = 10;
const RATE_LIMIT_WINDOW_MICROS: u64 = 60_000_000;

/// Disk-backed descriptor storage using redb.
pub struct RedbStorage {
    db: Database,
    rate_timestamps: HashMap<Vec<u8>, Vec<u64>>,
    /// When true, bypass per-publisher rate limits (used for internal seeding).
    pub skip_rate_limit: bool,
}

pub(crate) fn hash_bytes(h: &Hash) -> Vec<u8> {
    let mut bytes = vec![h.algorithm];
    bytes.extend_from_slice(&h.digest);
    bytes
}

pub(crate) fn identity_bytes(id: &Identity) -> Vec<u8> {
    let mut bytes = vec![id.algorithm];
    bytes.extend_from_slice(&id.public_key);
    bytes
}

fn dedup_key(desc: &Descriptor) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&identity_bytes(&desc.publisher));
    hasher.update(&hash_bytes(&desc.schema_hash));
    hasher.update(desc.topic.as_bytes());
    hasher.finalize().as_bytes().to_vec()
}

fn db_err(e: impl std::fmt::Display) -> StoreError {
    StoreError::ValidationFailed(format!("storage: {e}"))
}

impl RedbStorage {
    /// Open or create a redb-backed descriptor store.
    ///
    /// Sequence watermarks (replay protection) are persisted in the SEQUENCES
    /// table and read directly from disk during `store_descriptor_at`, so they
    /// survive process restarts without needing to be loaded into memory here.
    pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Database::create(path)?;
        // Initialize tables on first open
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(DESCRIPTORS)?;
            let _ = write_txn.open_multimap_table(BY_ROUTING_KEY)?;
            let _ = write_txn.open_table(DEDUP_INDEX)?;
            let _ = write_txn.open_table(SEQUENCES)?;
        }
        write_txn.commit()?;
        Ok(Self {
            db,
            rate_timestamps: HashMap::new(),
            skip_rate_limit: false,
        })
    }

    fn check_rate_limit(&mut self, publisher: &Identity, now_micros: u64) -> bool {
        let key = identity_bytes(publisher);
        let entries = self.rate_timestamps.entry(key).or_default();
        entries.retain(|&ts| now_micros.saturating_sub(ts) < RATE_LIMIT_WINDOW_MICROS);
        if entries.len() >= RATE_LIMIT_PER_MINUTE {
            return false;
        }
        entries.push(now_micros);
        true
    }
}

impl DescriptorStorage for RedbStorage {
    fn store_descriptor(&mut self, descriptor: Descriptor) -> Result<(), StoreError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        self.store_descriptor_at(descriptor, now)
    }

    fn store_descriptor_at(
        &mut self,
        descriptor: Descriptor,
        now_micros: u64,
    ) -> Result<(), StoreError> {
        // Validate (Section 2.2 steps 1-7)
        descriptor
            .validate(now_micros)
            .map_err(|e| StoreError::ValidationFailed(e.to_string()))?;

        // Rate limit (in-memory) — skipped for internal operations
        if !self.skip_rate_limit && !self.check_rate_limit(&descriptor.publisher, now_micros) {
            return Err(StoreError::RateLimited {
                limit: RATE_LIMIT_PER_MINUTE,
            });
        }

        let dk = dedup_key(&descriptor);
        let desc_id_bytes = hash_bytes(&descriptor.id);
        let desc_cbor = to_cbor(&descriptor).map_err(db_err)?;

        let write_txn = self.db.begin_write().map_err(db_err)?;

        // Sequence check
        {
            let seq_table = write_txn.open_table(SEQUENCES).map_err(db_err)?;
            if let Some(current) = seq_table.get(dk.as_slice()).map_err(db_err)? {
                let current_seq = current.value();
                if descriptor.sequence < current_seq {
                    return Err(StoreError::StaleDescriptor {
                        received: descriptor.sequence,
                        current: current_seq,
                    });
                }
            }
        }

        // Remove old version if exists (by dedup key).
        // Collect data in read-only pass, then mutate in a separate pass.
        let old_removal: Option<(Vec<u8>, Vec<Hash>)> = {
            let dedup_table = write_txn.open_table(DEDUP_INDEX).map_err(db_err)?;
            let old_id = dedup_table
                .get(dk.as_slice())
                .map_err(db_err)?
                .map(|g| g.value().to_vec());
            drop(dedup_table);
            if let Some(old_id_bytes) = old_id {
                let desc_table = write_txn.open_table(DESCRIPTORS).map_err(db_err)?;
                let routing_keys = desc_table
                    .get(old_id_bytes.as_slice())
                    .map_err(db_err)?
                    .and_then(|g| from_cbor::<Descriptor>(g.value()).ok())
                    .map(|d| d.routing_keys)
                    .unwrap_or_default();
                drop(desc_table);
                Some((old_id_bytes, routing_keys))
            } else {
                None
            }
        };

        if let Some((old_id_bytes, old_routing_keys)) = old_removal {
            {
                let mut rk_table =
                    write_txn.open_multimap_table(BY_ROUTING_KEY).map_err(db_err)?;
                for rk in &old_routing_keys {
                    let rk_bytes = hash_bytes(rk);
                    let _ = rk_table.remove(rk_bytes.as_slice(), old_id_bytes.as_slice());
                }
            }
            {
                let mut dt = write_txn.open_table(DESCRIPTORS).map_err(db_err)?;
                let _ = dt.remove(old_id_bytes.as_slice());
            }
        }

        // Store new descriptor
        {
            let mut desc_table = write_txn.open_table(DESCRIPTORS).map_err(db_err)?;
            desc_table
                .insert(desc_id_bytes.as_slice(), desc_cbor.as_slice())
                .map_err(db_err)?;
        }

        // Update routing key index
        {
            let mut rk_table = write_txn.open_multimap_table(BY_ROUTING_KEY).map_err(db_err)?;
            for rk in &descriptor.routing_keys {
                let rk_bytes = hash_bytes(rk);
                rk_table
                    .insert(rk_bytes.as_slice(), desc_id_bytes.as_slice())
                    .map_err(db_err)?;
            }
        }

        // Update dedup index
        {
            let mut dedup_table = write_txn.open_table(DEDUP_INDEX).map_err(db_err)?;
            dedup_table
                .insert(dk.as_slice(), desc_id_bytes.as_slice())
                .map_err(db_err)?;
        }

        // Update sequence
        {
            let mut seq_table = write_txn.open_table(SEQUENCES).map_err(db_err)?;
            seq_table
                .insert(dk.as_slice(), descriptor.sequence)
                .map_err(db_err)?;
        }

        write_txn.commit().map_err(db_err)?;
        Ok(())
    }

    fn get_descriptors(
        &self,
        routing_key: &Hash,
        filters: Option<&FilterSet>,
    ) -> Vec<Descriptor> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        self.get_descriptors_at(routing_key, filters, now)
    }

    fn get_descriptors_at(
        &self,
        routing_key: &Hash,
        filters: Option<&FilterSet>,
        now_micros: u64,
    ) -> Vec<Descriptor> {
        let Ok(read_txn) = self.db.begin_read() else {
            return Vec::new();
        };
        let rk_bytes = hash_bytes(routing_key);

        let Ok(rk_table) = read_txn.open_multimap_table(BY_ROUTING_KEY) else {
            return Vec::new();
        };
        let Ok(iter) = rk_table.get(rk_bytes.as_slice()) else {
            return Vec::new();
        };

        let Ok(desc_table) = read_txn.open_table(DESCRIPTORS) else {
            return Vec::new();
        };

        let mut descriptors = Vec::new();
        for entry in iter {
            let Ok(guard) = entry else { continue };
            let desc_id = guard.value();
            let Ok(Some(cbor_guard)) = desc_table.get(desc_id) else {
                continue;
            };
            let Ok(desc) = from_cbor::<Descriptor>(cbor_guard.value()) else {
                continue;
            };

            // Filter expired
            let effective_start = std::cmp::min(desc.timestamp, now_micros);
            let ttl_micros = u64::from(desc.ttl) * 1_000_000;
            if effective_start + ttl_micros <= now_micros {
                continue;
            }

            // Apply user filters
            if let Some(f) = filters {
                if let Some(ref sh) = f.schema_hash {
                    if &desc.schema_hash != sh {
                        continue;
                    }
                }
                if let Some(min_ts) = f.min_timestamp {
                    if desc.timestamp < min_ts {
                        continue;
                    }
                }
                if let Some(ref pub_id) = f.publisher {
                    if &desc.publisher != pub_id {
                        continue;
                    }
                }
            }

            descriptors.push(desc);
        }

        descriptors
    }

    fn evict_expired(&mut self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        self.evict_expired_at(now);
    }

    fn evict_expired_at(&mut self, now_micros: u64) {
        let Ok(write_txn) = self.db.begin_write() else {
            return;
        };

        // Collect expired descriptors
        let mut expired: Vec<Descriptor> = Vec::new();
        {
            let Ok(desc_table) = write_txn.open_table(DESCRIPTORS) else {
                return;
            };
            let Ok(iter) = desc_table.iter() else {
                return;
            };
            for entry in iter {
                let Ok((_, cbor_guard)) = entry else {
                    continue;
                };
                if let Ok(desc) = from_cbor::<Descriptor>(cbor_guard.value()) {
                    let effective_start = std::cmp::min(desc.timestamp, now_micros);
                    let ttl_micros = u64::from(desc.ttl) * 1_000_000;
                    if effective_start + ttl_micros <= now_micros {
                        expired.push(desc);
                    }
                }
            }
        }

        // Remove expired
        for desc in &expired {
            let desc_id_bytes = hash_bytes(&desc.id);
            let dk = dedup_key(desc);

            if let Ok(mut table) = write_txn.open_table(DESCRIPTORS) {
                let _ = table.remove(desc_id_bytes.as_slice());
            }

            if let Ok(mut table) = write_txn.open_multimap_table(BY_ROUTING_KEY) {
                for rk in &desc.routing_keys {
                    let rk_bytes = hash_bytes(rk);
                    let _ = table.remove(rk_bytes.as_slice(), desc_id_bytes.as_slice());
                }
            }

            if let Ok(mut table) = write_txn.open_table(DEDUP_INDEX) {
                let _ = table.remove(dk.as_slice());
            }

            if let Ok(mut table) = write_txn.open_table(SEQUENCES) {
                let _ = table.remove(dk.as_slice());
            }
        }

        let _ = write_txn.commit();
    }

    fn has_descriptors(&self, routing_key: &Hash) -> bool {
        let Ok(read_txn) = self.db.begin_read() else {
            return false;
        };
        let rk_bytes = hash_bytes(routing_key);
        let Ok(rk_table) = read_txn.open_multimap_table(BY_ROUTING_KEY) else {
            return false;
        };
        let Ok(mut iter) = rk_table.get(rk_bytes.as_slice()) else {
            return false;
        };
        iter.next().is_some()
    }

    fn descriptor_count(&self) -> usize {
        let Ok(read_txn) = self.db.begin_read() else {
            return 0;
        };
        let Ok(table) = read_txn.open_table(DESCRIPTORS) else {
            return 0;
        };
        table.len().unwrap_or(0) as usize
    }

    fn routing_key_count(&self) -> usize {
        let Ok(read_txn) = self.db.begin_read() else {
            return 0;
        };
        let Ok(table) = read_txn.open_multimap_table(BY_ROUTING_KEY) else {
            return 0;
        };
        table.len().unwrap_or(0) as usize
    }
}

impl RedbStorage {
    /// Return all non-expired descriptors in the store (for gossip replication).
    pub fn all_descriptors(&self) -> Vec<Descriptor> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let Ok(read_txn) = self.db.begin_read() else {
            return Vec::new();
        };
        let Ok(table) = read_txn.open_table(DESCRIPTORS) else {
            return Vec::new();
        };

        let mut descriptors = Vec::new();
        let Ok(iter) = table.iter() else {
            return Vec::new();
        };

        for entry in iter {
            let Ok((_key, value)) = entry else { continue };
            let Ok(desc) = from_cbor::<Descriptor>(value.value()) else {
                continue;
            };
            let effective_start = std::cmp::min(desc.timestamp, now);
            let ttl_micros = u64::from(desc.ttl) * 1_000_000;
            if effective_start + ttl_micros <= now {
                continue;
            }
            descriptors.push(desc);
        }
        descriptors
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

    #[test]
    fn store_and_retrieve() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = RedbStorage::open(&dir.path().join("test.redb")).unwrap();
        let kp = Keypair::generate();
        let desc = make_descriptor(&kp, "topic", 1, 3600);
        let rk = routing_key("compute/inference/text-generation");

        store.store_descriptor(desc.clone()).unwrap();

        let results = store.get_descriptors(&rk, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, desc.id);
    }

    #[test]
    fn sequence_replacement() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = RedbStorage::open(&dir.path().join("test.redb")).unwrap();
        let kp = Keypair::generate();
        let rk = routing_key("compute/inference/text-generation");

        store.store_descriptor(make_descriptor(&kp, "topic", 1, 3600)).unwrap();
        store.store_descriptor(make_descriptor(&kp, "topic", 2, 3600)).unwrap();

        let results = store.get_descriptors(&rk, None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].sequence, 2);
    }

    #[test]
    fn stale_sequence_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = RedbStorage::open(&dir.path().join("test.redb")).unwrap();
        let kp = Keypair::generate();

        store.store_descriptor(make_descriptor(&kp, "topic", 2, 3600)).unwrap();
        let result = store.store_descriptor(make_descriptor(&kp, "topic", 1, 3600));
        assert!(matches!(result, Err(StoreError::StaleDescriptor { .. })));
    }

    #[test]
    fn persistence_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.redb");
        let kp = Keypair::generate();
        let rk = routing_key("compute/inference/text-generation");

        // Store
        {
            let mut store = RedbStorage::open(&db_path).unwrap();
            store.store_descriptor(make_descriptor(&kp, "topic", 1, 3600)).unwrap();
        }

        // Reopen and verify
        {
            let store = RedbStorage::open(&db_path).unwrap();
            let results = store.get_descriptors(&rk, None);
            assert_eq!(results.len(), 1);
        }
    }

    /// B4: Replay watermark persistence — sequence floors survive reopen.
    ///
    /// The SEQUENCES table persists the highest sequence seen per dedup key.
    /// On reopen, store_descriptor_at reads directly from the redb table
    /// within the write transaction, so stale descriptors are rejected even
    /// after the process restarts.
    #[test]
    fn replay_watermark_persisted_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("watermark.redb");
        let kp = Keypair::generate();
        let now = now_micros();

        // Store descriptor with sequence=5, then drop the store
        {
            let mut store = RedbStorage::open(&db_path).unwrap();
            let desc = Descriptor::create(
                &kp,
                schema_hash("core/capability"),
                "topic".into(),
                b"payload".to_vec(),
                now,
                5,
                3600,
                vec![routing_key("compute/inference/text-generation")],
            )
            .unwrap();
            store.store_descriptor_at(desc, now).unwrap();
        }

        // Reopen and try to store descriptor with sequence=3 — should be rejected
        {
            let mut store = RedbStorage::open(&db_path).unwrap();
            let desc = Descriptor::create(
                &kp,
                schema_hash("core/capability"),
                "topic".into(),
                b"payload v2".to_vec(),
                now,
                3,
                3600,
                vec![routing_key("compute/inference/text-generation")],
            )
            .unwrap();
            let result = store.store_descriptor_at(desc, now);
            assert!(
                matches!(result, Err(StoreError::StaleDescriptor { received: 3, current: 5 })),
                "stale descriptor should be rejected after reopen: {:?}",
                result,
            );
        }

        // Also verify that a higher sequence still works after reopen
        {
            let mut store = RedbStorage::open(&db_path).unwrap();
            let desc = Descriptor::create(
                &kp,
                schema_hash("core/capability"),
                "topic".into(),
                b"payload v3".to_vec(),
                now,
                6,
                3600,
                vec![routing_key("compute/inference/text-generation")],
            )
            .unwrap();
            assert!(store.store_descriptor_at(desc, now).is_ok());
        }
    }

    #[test]
    fn evict_expired() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = RedbStorage::open(&dir.path().join("test.redb")).unwrap();
        let kp = Keypair::generate();
        let rk = routing_key("compute/inference/text-generation");
        let now = now_micros();

        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            60, // 60s TTL
            vec![rk.clone()],
        )
        .unwrap();
        store.store_descriptor_at(desc, now).unwrap();

        // Not expired yet
        assert_eq!(store.get_descriptors_at(&rk, None, now + 30_000_000).len(), 1);

        // Expired
        store.evict_expired_at(now + 61_000_000);
        assert_eq!(store.descriptor_count(), 0);
    }
}
