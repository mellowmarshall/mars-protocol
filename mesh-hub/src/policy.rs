//! Policy engine — blocklists, store mode, access control, quota enforcement.

use crate::config::{PolicyConfig, StoreMode};
use crate::tenant::Tenant;
use mesh_core::Descriptor;

/// Stateless policy checks for incoming descriptors.
pub struct PolicyEngine {
    config: PolicyConfig,
}

impl PolicyEngine {
    pub fn new(config: PolicyConfig) -> Self {
        Self { config }
    }

    /// Check if a descriptor is allowed by policy.
    ///
    /// `is_tenant` indicates whether the publisher is a registered tenant identity.
    /// Returns `Err` with a generic reason if rejected (no policy details leaked).
    pub fn check_store(&self, descriptor: &Descriptor, is_tenant: bool) -> Result<(), String> {
        // Store mode restrictions
        match self.config.store_mode {
            StoreMode::TenantOnly | StoreMode::Allowlist if !is_tenant => {
                return Err("policy".into());
            }
            _ => {}
        }

        // Blocked identities (matched by DID)
        let publisher_did = descriptor.publisher.did();
        if self
            .config
            .blocked_identities
            .iter()
            .any(|b| b == &publisher_did)
        {
            return Err("policy".into());
        }

        // Blocked routing keys (matched by hex digest)
        for rk in &descriptor.routing_keys {
            let rk_hex = hex::encode(&rk.digest);
            if self
                .config
                .blocked_routing_keys
                .iter()
                .any(|b| b == &rk_hex)
            {
                return Err("policy".into());
            }
        }

        Ok(())
    }

    /// Check tenant quota limits for storing a descriptor.
    ///
    /// Returns `Err` if the tenant has exceeded their descriptor or storage quotas.
    pub fn check_quotas(&self, tenant: &Tenant, descriptor_size: usize) -> Result<(), String> {
        if tenant.current_descriptors >= tenant.max_descriptors {
            return Err("policy".into());
        }
        if tenant.current_bytes + descriptor_size as u64 > tenant.max_storage_bytes {
            return Err("policy".into());
        }
        Ok(())
    }

    /// Check that a tenant has sufficient MU budget for an operation.
    pub fn check_mu_budget(&self, tenant: &Tenant, cost: i64) -> Result<(), String> {
        if tenant.mu_balance < cost {
            return Err("policy".into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PolicyConfig;
    use mesh_core::hash::schema_hash;
    use mesh_core::identity::Keypair;
    use mesh_core::routing::routing_key;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_descriptor() -> Descriptor {
        let kp = Keypair::generate();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![routing_key("compute")],
        )
        .unwrap()
    }

    fn make_tenant(descriptors: u64, bytes: u64, mu_balance: i64) -> Tenant {
        Tenant {
            id: uuid::Uuid::new_v4(),
            name: "test".into(),
            tier: "free".into(),
            max_descriptors: 100,
            max_storage_bytes: 1_048_576,
            max_query_rate: 10,
            max_store_rate: 1,
            current_descriptors: descriptors,
            current_bytes: bytes,
            mu_balance,
            mu_limit: 10_000,
            created_at: 0,
        }
    }

    #[test]
    fn open_mode_allows_all() {
        let engine = PolicyEngine::new(PolicyConfig::default());
        let desc = make_descriptor();
        assert!(engine.check_store(&desc, false).is_ok());
        assert!(engine.check_store(&desc, true).is_ok());
    }

    #[test]
    fn tenant_only_rejects_non_tenant() {
        let config = PolicyConfig {
            store_mode: StoreMode::TenantOnly,
            ..Default::default()
        };
        let engine = PolicyEngine::new(config);
        let desc = make_descriptor();
        assert!(engine.check_store(&desc, false).is_err());
        assert!(engine.check_store(&desc, true).is_ok());
    }

    #[test]
    fn blocked_identity() {
        let kp = Keypair::generate();
        let did = kp.identity().did();
        let config = PolicyConfig {
            blocked_identities: vec![did],
            ..Default::default()
        };
        let engine = PolicyEngine::new(config);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![routing_key("compute")],
        )
        .unwrap();

        assert!(engine.check_store(&desc, true).is_err());
    }

    // ── Quota enforcement tests ──

    #[test]
    fn quota_under_limit() {
        let engine = PolicyEngine::new(PolicyConfig::default());
        let tenant = make_tenant(50, 500_000, 5_000);
        assert!(engine.check_quotas(&tenant, 1024).is_ok());
    }

    #[test]
    fn quota_at_descriptor_limit() {
        let engine = PolicyEngine::new(PolicyConfig::default());
        let tenant = make_tenant(100, 0, 10_000); // at max_descriptors
        assert!(engine.check_quotas(&tenant, 1024).is_err());
    }

    #[test]
    fn quota_over_storage_limit() {
        let engine = PolicyEngine::new(PolicyConfig::default());
        let tenant = make_tenant(50, 1_048_000, 10_000); // close to max
        // Adding 1024 bytes would exceed 1_048_576
        assert!(engine.check_quotas(&tenant, 1024).is_err());
    }

    #[test]
    fn quota_storage_exactly_at_limit() {
        let engine = PolicyEngine::new(PolicyConfig::default());
        // current_bytes + descriptor_size == max_storage_bytes exactly
        let tenant = make_tenant(50, 1_048_576 - 1024, 10_000);
        // Adding exactly 1024 would hit limit but not exceed
        assert!(engine.check_quotas(&tenant, 1024).is_ok());
    }

    #[test]
    fn mu_budget_sufficient() {
        let engine = PolicyEngine::new(PolicyConfig::default());
        let tenant = make_tenant(0, 0, 100);
        assert!(engine.check_mu_budget(&tenant, 10).is_ok());
    }

    #[test]
    fn mu_budget_insufficient() {
        let engine = PolicyEngine::new(PolicyConfig::default());
        let tenant = make_tenant(0, 0, 5);
        assert!(engine.check_mu_budget(&tenant, 10).is_err());
    }

    #[test]
    fn mu_budget_zero_balance() {
        let engine = PolicyEngine::new(PolicyConfig::default());
        let tenant = make_tenant(0, 0, 0);
        assert!(engine.check_mu_budget(&tenant, 1).is_err());
        // Zero cost should pass even with zero balance
        assert!(engine.check_mu_budget(&tenant, 0).is_ok());
    }
}
