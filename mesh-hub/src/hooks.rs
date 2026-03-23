//! Hub protocol hook — policy enforcement at the DHT layer.

use std::sync::{Arc, Mutex};

use mesh_core::{Descriptor, Hash};
use mesh_dht::ProtocolHook;

use crate::policy::PolicyEngine;
use crate::storage::redb::identity_bytes;
use crate::tenant::TenantManager;

/// Protocol hook that enforces hub policy on incoming DHT operations.
pub struct HubProtocolHook {
    policy: PolicyEngine,
    tenant_manager: Arc<Mutex<TenantManager>>,
}

impl HubProtocolHook {
    pub fn new(policy: PolicyEngine, tenant_manager: Arc<Mutex<TenantManager>>) -> Self {
        Self {
            policy,
            tenant_manager,
        }
    }
}

impl ProtocolHook for HubProtocolHook {
    fn pre_store(&self, descriptor: &Descriptor) -> Result<(), String> {
        let id_bytes = identity_bytes(&descriptor.publisher);
        let is_tenant = self
            .tenant_manager
            .lock()
            .unwrap()
            .find_tenant_by_identity(&id_bytes)
            .ok()
            .flatten()
            .is_some();
        self.policy.check_store(descriptor, is_tenant)
    }

    fn post_store(&self, descriptor: &Descriptor) {
        tracing::info!(
            descriptor_id = %hex::encode(&descriptor.id.digest),
            publisher = %descriptor.publisher.did(),
            "descriptor stored"
        );
    }

    fn pre_query(&self, _routing_key: &Hash) -> Result<(), String> {
        Ok(())
    }

    fn post_query(&self, routing_key: &Hash, result_count: usize) {
        tracing::debug!(
            routing_key = %hex::encode(&routing_key.digest),
            result_count,
            "query completed"
        );
    }
}
