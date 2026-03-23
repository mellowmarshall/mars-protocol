//! Policy engine — blocklists, store mode, access control.

use crate::config::{PolicyConfig, StoreMode};
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
}
