//! Storage layer — L1 hot cache wrapping L2 redb backend.

pub mod redb;

use mesh_core::message::FilterSet;
use mesh_core::{Descriptor, Hash};
use mesh_dht::storage::{DescriptorStorage, StoreError};
use moka::sync::Cache;

use self::redb::hash_bytes;

/// L1 hot cache wrapping any `DescriptorStorage` backend.
///
/// Caches descriptor lists by routing key. Invalidates on writes.
pub struct CachedStorage<S: DescriptorStorage> {
    inner: S,
    cache: Cache<Vec<u8>, Vec<Descriptor>>,
}

impl<S: DescriptorStorage> CachedStorage<S> {
    pub fn new(inner: S, max_entries: u64) -> Self {
        let cache = Cache::builder().max_capacity(max_entries).build();
        Self { inner, cache }
    }

    pub fn inner(&self) -> &S {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }
}

impl<S: DescriptorStorage> DescriptorStorage for CachedStorage<S> {
    fn store_descriptor(&mut self, descriptor: Descriptor) -> Result<(), StoreError> {
        let rk_keys: Vec<Vec<u8>> = descriptor.routing_keys.iter().map(hash_bytes).collect();
        self.inner.store_descriptor(descriptor)?;
        for rk in rk_keys {
            self.cache.invalidate(&rk);
        }
        Ok(())
    }

    fn store_descriptor_at(
        &mut self,
        descriptor: Descriptor,
        now_micros: u64,
    ) -> Result<(), StoreError> {
        let rk_keys: Vec<Vec<u8>> = descriptor.routing_keys.iter().map(hash_bytes).collect();
        self.inner.store_descriptor_at(descriptor, now_micros)?;
        for rk in rk_keys {
            self.cache.invalidate(&rk);
        }
        Ok(())
    }

    fn get_descriptors(
        &self,
        routing_key: &Hash,
        filters: Option<&FilterSet>,
    ) -> Vec<Descriptor> {
        let rk_bytes = hash_bytes(routing_key);

        // Cache unfiltered results
        let all = if let Some(cached) = self.cache.get(&rk_bytes) {
            cached
        } else {
            let loaded = self.inner.get_descriptors(routing_key, None);
            self.cache.insert(rk_bytes, loaded.clone());
            loaded
        };

        // Apply filters post-cache
        if let Some(f) = filters {
            all.into_iter()
                .filter(|d| {
                    if let Some(ref sh) = f.schema_hash {
                        if &d.schema_hash != sh {
                            return false;
                        }
                    }
                    if let Some(min_ts) = f.min_timestamp {
                        if d.timestamp < min_ts {
                            return false;
                        }
                    }
                    if let Some(ref pub_id) = f.publisher {
                        if &d.publisher != pub_id {
                            return false;
                        }
                    }
                    true
                })
                .collect()
        } else {
            all
        }
    }

    fn get_descriptors_at(
        &self,
        routing_key: &Hash,
        filters: Option<&FilterSet>,
        now_micros: u64,
    ) -> Vec<Descriptor> {
        // Skip cache for timestamped queries (testing)
        self.inner
            .get_descriptors_at(routing_key, filters, now_micros)
    }

    fn evict_expired(&mut self) {
        self.inner.evict_expired();
        self.cache.invalidate_all();
    }

    fn evict_expired_at(&mut self, now_micros: u64) {
        self.inner.evict_expired_at(now_micros);
        self.cache.invalidate_all();
    }

    fn has_descriptors(&self, routing_key: &Hash) -> bool {
        let rk_bytes = hash_bytes(routing_key);
        if let Some(cached) = self.cache.get(&rk_bytes) {
            !cached.is_empty()
        } else {
            self.inner.has_descriptors(routing_key)
        }
    }

    fn descriptor_count(&self) -> usize {
        self.inner.descriptor_count()
    }

    fn routing_key_count(&self) -> usize {
        self.inner.routing_key_count()
    }
}
