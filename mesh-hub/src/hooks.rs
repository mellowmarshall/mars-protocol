//! Hub protocol hook — policy enforcement and rate limiting at the DHT layer.

use std::sync::{Arc, Mutex};

use mesh_core::{Descriptor, Hash};
use mesh_dht::ProtocolHook;

use crate::metrics::HubMetrics;
use crate::policy::PolicyEngine;
use crate::rate_limit::{HubRateLimiter, Operation};
use crate::storage::redb::identity_bytes;
use crate::tenant::TenantManager;

/// Protocol hook that enforces hub policy and rate limits on incoming DHT operations.
pub struct HubProtocolHook {
    policy: PolicyEngine,
    tenant_manager: Arc<Mutex<TenantManager>>,
    rate_limiter: Arc<HubRateLimiter>,
    metrics: Option<HubMetrics>,
}

impl HubProtocolHook {
    pub fn new(
        policy: PolicyEngine,
        tenant_manager: Arc<Mutex<TenantManager>>,
        rate_limiter: Arc<HubRateLimiter>,
    ) -> Self {
        Self {
            policy,
            tenant_manager,
            rate_limiter,
            metrics: None,
        }
    }

    /// Attach metrics to this hook so it can record rate-limit events.
    pub fn with_metrics(mut self, metrics: HubMetrics) -> Self {
        self.metrics = Some(metrics);
        self
    }
}

impl ProtocolHook for HubProtocolHook {
    fn pre_store(&self, descriptor: &Descriptor) -> Result<(), String> {
        // Rate limit check (identity-based).
        // TODO: Add IP-based rate limiting once IP context is available in the hook interface.
        if let Err(e) = self
            .rate_limiter
            .check_identity(&descriptor.publisher, Operation::Store)
        {
            tracing::warn!(publisher = %descriptor.publisher.did(), %e, "store rate limited");
            if let Some(ref m) = self.metrics {
                m.record_rate_limited("store");
            }
            return Err("rate limited".into());
        }

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
        // TODO: Add identity-based query rate limiting once the pre_query interface
        // includes the requesting identity. Currently pre_query only receives the
        // routing key, so identity-based rate limiting is not possible here.
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
