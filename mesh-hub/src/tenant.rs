//! Multi-tenant management with SQLite backend (L3).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::auth::DIDAuthChallenge;

/// A hub tenant account.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Tenant {
    pub id: Uuid,
    pub name: String,
    pub tier: String,
    pub max_descriptors: u64,
    pub max_storage_bytes: u64,
    pub max_query_rate: u32,
    pub max_store_rate: u32,
    pub current_descriptors: u64,
    pub current_bytes: u64,
    pub mu_balance: i64,
    pub mu_limit: i64,
    pub created_at: u64,
}

/// Tenant usage summary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TenantUsage {
    pub current_descriptors: u64,
    pub current_bytes: u64,
    pub mu_balance: i64,
    pub mu_limit: i64,
}

/// Error type for MU operations.
#[derive(Debug, thiserror::Error)]
pub enum MuError {
    #[error("insufficient MU balance: have {balance}, need {cost}")]
    InsufficientBalance { balance: i64, cost: i64 },
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

/// Manages tenant accounts and identity registrations in SQLite.
pub struct TenantManager {
    conn: Connection,
}

impl TenantManager {
    /// Open or create the tenant database.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tenants (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                tier TEXT NOT NULL DEFAULT 'free',
                max_descriptors INTEGER NOT NULL DEFAULT 100,
                max_storage_bytes INTEGER NOT NULL DEFAULT 1048576,
                max_query_rate INTEGER NOT NULL DEFAULT 10,
                max_store_rate INTEGER NOT NULL DEFAULT 1,
                current_descriptors INTEGER NOT NULL DEFAULT 0,
                current_bytes INTEGER NOT NULL DEFAULT 0,
                mu_balance INTEGER NOT NULL DEFAULT 100000,
                mu_limit INTEGER NOT NULL DEFAULT 100000,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tenant_identities (
                tenant_id TEXT NOT NULL,
                identity_bytes BLOB NOT NULL,
                did TEXT NOT NULL,
                registered_at INTEGER NOT NULL,
                PRIMARY KEY (tenant_id, identity_bytes),
                FOREIGN KEY (tenant_id) REFERENCES tenants(id)
            );
            CREATE TABLE IF NOT EXISTS did_auth_challenges (
                id TEXT PRIMARY KEY,
                tenant_id TEXT NOT NULL,
                nonce BLOB NOT NULL,
                hub_did TEXT NOT NULL,
                action TEXT NOT NULL,
                issued_at INTEGER NOT NULL,
                expiry INTEGER NOT NULL,
                consumed INTEGER NOT NULL DEFAULT 0
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn create_tenant(&self, name: &str, tier: &str) -> Result<Tenant, rusqlite::Error> {
        let id = Uuid::new_v4();
        let now = now_micros();
        let quotas = tier_quotas(tier);

        self.conn.execute(
            "INSERT INTO tenants (id, name, tier, max_descriptors, max_storage_bytes, \
             max_query_rate, max_store_rate, mu_balance, mu_limit, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id.to_string(),
                name,
                tier,
                quotas.max_descriptors as i64,
                quotas.max_storage_bytes as i64,
                quotas.max_query_rate,
                quotas.max_store_rate,
                quotas.mu_limit,
                quotas.mu_limit,
                now as i64
            ],
        )?;

        Ok(Tenant {
            id,
            name: name.into(),
            tier: tier.into(),
            max_descriptors: quotas.max_descriptors,
            max_storage_bytes: quotas.max_storage_bytes,
            max_query_rate: quotas.max_query_rate,
            max_store_rate: quotas.max_store_rate,
            current_descriptors: 0,
            current_bytes: 0,
            mu_balance: quotas.mu_limit,
            mu_limit: quotas.mu_limit,
            created_at: now,
        })
    }

    pub fn get_tenant(&self, id: &Uuid) -> Result<Option<Tenant>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tier, max_descriptors, max_storage_bytes, max_query_rate, \
             max_store_rate, current_descriptors, current_bytes, mu_balance, mu_limit, \
             created_at FROM tenants WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id.to_string()])?;
        match rows.next()? {
            Some(row) => Ok(Some(row_to_tenant(row)?)),
            None => Ok(None),
        }
    }

    pub fn list_tenants(&self) -> Result<Vec<Tenant>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tier, max_descriptors, max_storage_bytes, max_query_rate, \
             max_store_rate, current_descriptors, current_bytes, mu_balance, mu_limit, \
             created_at FROM tenants",
        )?;
        let rows = stmt.query_map([], row_to_tenant)?;
        rows.collect()
    }

    pub fn delete_tenant(&self, id: &Uuid) -> Result<bool, rusqlite::Error> {
        let id_str = id.to_string();
        self.conn.execute(
            "DELETE FROM tenant_identities WHERE tenant_id = ?1",
            params![id_str],
        )?;
        let count = self
            .conn
            .execute("DELETE FROM tenants WHERE id = ?1", params![id_str])?;
        Ok(count > 0)
    }

    pub fn register_identity(
        &self,
        tenant_id: &Uuid,
        identity_bytes: &[u8],
        did: &str,
    ) -> Result<(), rusqlite::Error> {
        let now = now_micros();
        self.conn.execute(
            "INSERT OR REPLACE INTO tenant_identities \
             (tenant_id, identity_bytes, did, registered_at) VALUES (?1, ?2, ?3, ?4)",
            params![tenant_id.to_string(), identity_bytes, did, now as i64],
        )?;
        Ok(())
    }

    pub fn remove_identity(&self, tenant_id: &Uuid, did: &str) -> Result<bool, rusqlite::Error> {
        let count = self.conn.execute(
            "DELETE FROM tenant_identities WHERE tenant_id = ?1 AND did = ?2",
            params![tenant_id.to_string(), did],
        )?;
        Ok(count > 0)
    }

    /// Find which tenant owns a given identity (by raw identity bytes).
    pub fn find_tenant_by_identity(
        &self,
        identity_bytes: &[u8],
    ) -> Result<Option<Uuid>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT tenant_id FROM tenant_identities WHERE identity_bytes = ?1",
        )?;
        let mut rows = stmt.query(params![identity_bytes])?;
        match rows.next()? {
            Some(row) => {
                let id_str: String = row.get(0)?;
                Ok(Some(Uuid::parse_str(&id_str).unwrap()))
            }
            None => Ok(None),
        }
    }

    // ── MU Metering ──

    /// Atomically deduct MU from a tenant's balance.
    /// Returns an error if the tenant has insufficient balance.
    pub fn deduct_mu(&self, tenant_id: &Uuid, cost: i64) -> Result<(), MuError> {
        let id_str = tenant_id.to_string();

        // Read current balance
        let balance: i64 = self.conn.query_row(
            "SELECT mu_balance FROM tenants WHERE id = ?1",
            params![id_str],
            |row| row.get(0),
        )?;

        if balance < cost {
            return Err(MuError::InsufficientBalance { balance, cost });
        }

        self.conn.execute(
            "UPDATE tenants SET mu_balance = mu_balance - ?1 WHERE id = ?2",
            params![cost, id_str],
        )?;
        Ok(())
    }

    /// Get usage summary for a tenant.
    pub fn get_usage(&self, tenant_id: &Uuid) -> Result<TenantUsage, rusqlite::Error> {
        let id_str = tenant_id.to_string();
        self.conn.query_row(
            "SELECT current_descriptors, current_bytes, mu_balance, mu_limit \
             FROM tenants WHERE id = ?1",
            params![id_str],
            |row| {
                Ok(TenantUsage {
                    current_descriptors: row.get::<_, i64>(0)? as u64,
                    current_bytes: row.get::<_, i64>(1)? as u64,
                    mu_balance: row.get(2)?,
                    mu_limit: row.get(3)?,
                })
            },
        )
    }

    /// Update quota limits for a tenant.
    pub fn update_quotas(
        &self,
        tenant_id: &Uuid,
        max_descriptors: Option<u64>,
        max_storage_bytes: Option<u64>,
        mu_limit: Option<i64>,
    ) -> Result<(), rusqlite::Error> {
        let id_str = tenant_id.to_string();
        if let Some(v) = max_descriptors {
            self.conn.execute(
                "UPDATE tenants SET max_descriptors = ?1 WHERE id = ?2",
                params![v as i64, id_str],
            )?;
        }
        if let Some(v) = max_storage_bytes {
            self.conn.execute(
                "UPDATE tenants SET max_storage_bytes = ?1 WHERE id = ?2",
                params![v as i64, id_str],
            )?;
        }
        if let Some(v) = mu_limit {
            self.conn.execute(
                "UPDATE tenants SET mu_limit = ?1 WHERE id = ?2",
                params![v, id_str],
            )?;
        }
        Ok(())
    }

    // ── DID-Auth Challenge Storage ──

    /// Create and store a new DID-Auth challenge for a tenant.
    pub fn create_challenge(
        &self,
        tenant_id: &Uuid,
        hub_did: &str,
        action: &str,
    ) -> Result<DIDAuthChallenge, rusqlite::Error> {
        let challenge = DIDAuthChallenge::new(hub_did, action);
        self.conn.execute(
            "INSERT INTO did_auth_challenges \
             (id, tenant_id, nonce, hub_did, action, issued_at, expiry, consumed) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            params![
                challenge.id.to_string(),
                tenant_id.to_string(),
                challenge.nonce.as_slice(),
                &challenge.hub_did,
                &challenge.action,
                challenge.issued_at as i64,
                challenge.expiry as i64,
            ],
        )?;
        Ok(challenge)
    }

    /// Retrieve a stored challenge by ID.
    pub fn get_challenge(&self, id: &Uuid) -> Result<Option<DIDAuthChallenge>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, nonce, hub_did, action, issued_at, expiry, consumed \
             FROM did_auth_challenges WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id.to_string()])?;
        match rows.next()? {
            Some(row) => {
                let consumed: i32 = row.get(6)?;
                if consumed != 0 {
                    return Ok(None); // treat consumed challenges as not found
                }
                let nonce_bytes: Vec<u8> = row.get(1)?;
                let mut nonce = [0u8; 32];
                nonce.copy_from_slice(&nonce_bytes);
                Ok(Some(DIDAuthChallenge {
                    id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
                    nonce,
                    hub_did: row.get(2)?,
                    action: row.get(3)?,
                    issued_at: row.get::<_, i64>(4)? as u64,
                    expiry: row.get::<_, i64>(5)? as u64,
                }))
            }
            None => Ok(None),
        }
    }

    /// Consume a challenge (mark it as used). Rejects if already consumed.
    pub fn consume_challenge(&self, id: &Uuid) -> Result<(), String> {
        let id_str = id.to_string();

        // Check current state
        let consumed: i32 = self
            .conn
            .query_row(
                "SELECT consumed FROM did_auth_challenges WHERE id = ?1",
                params![id_str],
                |row| row.get(0),
            )
            .map_err(|e| format!("challenge not found: {e}"))?;

        if consumed != 0 {
            return Err("challenge already consumed".into());
        }

        self.conn
            .execute(
                "UPDATE did_auth_challenges SET consumed = 1 WHERE id = ?1",
                params![id_str],
            )
            .map_err(|e| format!("failed to consume challenge: {e}"))?;

        Ok(())
    }
}

fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Quotas returned by tier_quotas.
struct TierQuotas {
    max_descriptors: u64,
    max_storage_bytes: u64,
    max_query_rate: u32,
    max_store_rate: u32,
    mu_limit: i64,
}

fn tier_quotas(tier: &str) -> TierQuotas {
    match tier {
        "starter" => TierQuotas {
            max_descriptors: 1_000,
            max_storage_bytes: 10_485_760,
            max_query_rate: 50,
            max_store_rate: 5,
            mu_limit: 100_000,
        },
        "pro" => TierQuotas {
            max_descriptors: 10_000,
            max_storage_bytes: 104_857_600,
            max_query_rate: 100,
            max_store_rate: 10,
            mu_limit: 1_000_000,
        },
        "enterprise" => TierQuotas {
            max_descriptors: 1_000_000,
            max_storage_bytes: 10_737_418_240,
            max_query_rate: 1000,
            max_store_rate: 100,
            mu_limit: 10_000_000,
        },
        // "free" and anything else
        _ => TierQuotas {
            max_descriptors: 100,
            max_storage_bytes: 1_048_576,
            max_query_rate: 10,
            max_store_rate: 1,
            mu_limit: 10_000,
        },
    }
}

fn row_to_tenant(row: &rusqlite::Row<'_>) -> Result<Tenant, rusqlite::Error> {
    Ok(Tenant {
        id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap(),
        name: row.get(1)?,
        tier: row.get(2)?,
        max_descriptors: row.get::<_, i64>(3)? as u64,
        max_storage_bytes: row.get::<_, i64>(4)? as u64,
        max_query_rate: row.get::<_, i32>(5)? as u32,
        max_store_rate: row.get::<_, i32>(6)? as u32,
        current_descriptors: row.get::<_, i64>(7)? as u64,
        current_bytes: row.get::<_, i64>(8)? as u64,
        mu_balance: row.get(9)?,
        mu_limit: row.get(10)?,
        created_at: row.get::<_, i64>(11)? as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_crud() {
        let dir = tempfile::tempdir().unwrap();
        let tm = TenantManager::open(&dir.path().join("tenants.db")).unwrap();

        // Create
        let tenant = tm.create_tenant("test-org", "free").unwrap();
        assert_eq!(tenant.name, "test-org");
        assert_eq!(tenant.tier, "free");
        assert_eq!(tenant.max_descriptors, 100);
        assert_eq!(tenant.mu_balance, 10_000);
        assert_eq!(tenant.mu_limit, 10_000);

        // Get
        let loaded = tm.get_tenant(&tenant.id).unwrap().unwrap();
        assert_eq!(loaded.id, tenant.id);
        assert_eq!(loaded.mu_balance, 10_000);

        // List
        let all = tm.list_tenants().unwrap();
        assert_eq!(all.len(), 1);

        // Delete
        assert!(tm.delete_tenant(&tenant.id).unwrap());
        assert!(tm.get_tenant(&tenant.id).unwrap().is_none());
    }

    #[test]
    fn identity_registration() {
        let dir = tempfile::tempdir().unwrap();
        let tm = TenantManager::open(&dir.path().join("tenants.db")).unwrap();

        let tenant = tm.create_tenant("test-org", "free").unwrap();
        let id_bytes = b"test-identity-bytes";
        let did = "did:mesh:zTestDid";

        // Register
        tm.register_identity(&tenant.id, id_bytes, did).unwrap();

        // Find
        let found = tm.find_tenant_by_identity(id_bytes).unwrap();
        assert_eq!(found, Some(tenant.id));

        // Remove
        assert!(tm.remove_identity(&tenant.id, did).unwrap());
        assert!(tm.find_tenant_by_identity(id_bytes).unwrap().is_none());
    }

    #[test]
    fn mu_deduction_success() {
        let dir = tempfile::tempdir().unwrap();
        let tm = TenantManager::open(&dir.path().join("tenants.db")).unwrap();

        let tenant = tm.create_tenant("test-org", "free").unwrap();
        assert_eq!(tenant.mu_balance, 10_000);

        // Deduct some MU
        tm.deduct_mu(&tenant.id, 100).unwrap();
        let usage = tm.get_usage(&tenant.id).unwrap();
        assert_eq!(usage.mu_balance, 9_900);

        // Deduct more
        tm.deduct_mu(&tenant.id, 9_900).unwrap();
        let usage = tm.get_usage(&tenant.id).unwrap();
        assert_eq!(usage.mu_balance, 0);
    }

    #[test]
    fn mu_deduction_insufficient_balance() {
        let dir = tempfile::tempdir().unwrap();
        let tm = TenantManager::open(&dir.path().join("tenants.db")).unwrap();

        let tenant = tm.create_tenant("test-org", "free").unwrap();

        // Try to deduct more than balance
        let result = tm.deduct_mu(&tenant.id, 20_000);
        assert!(result.is_err());
        match result.unwrap_err() {
            MuError::InsufficientBalance { balance, cost } => {
                assert_eq!(balance, 10_000);
                assert_eq!(cost, 20_000);
            }
            e => panic!("expected InsufficientBalance, got: {e}"),
        }

        // Balance should be unchanged
        let usage = tm.get_usage(&tenant.id).unwrap();
        assert_eq!(usage.mu_balance, 10_000);
    }

    #[test]
    fn tier_mu_limits() {
        let dir = tempfile::tempdir().unwrap();
        let tm = TenantManager::open(&dir.path().join("tenants.db")).unwrap();

        let free = tm.create_tenant("free-org", "free").unwrap();
        assert_eq!(free.mu_limit, 10_000);

        let starter = tm.create_tenant("starter-org", "starter").unwrap();
        assert_eq!(starter.mu_limit, 100_000);

        let pro = tm.create_tenant("pro-org", "pro").unwrap();
        assert_eq!(pro.mu_limit, 1_000_000);

        let enterprise = tm.create_tenant("enterprise-org", "enterprise").unwrap();
        assert_eq!(enterprise.mu_limit, 10_000_000);
    }

    #[test]
    fn update_quotas() {
        let dir = tempfile::tempdir().unwrap();
        let tm = TenantManager::open(&dir.path().join("tenants.db")).unwrap();

        let tenant = tm.create_tenant("test-org", "free").unwrap();
        assert_eq!(tenant.max_descriptors, 100);

        tm.update_quotas(&tenant.id, Some(500), Some(5_000_000), Some(50_000))
            .unwrap();
        let updated = tm.get_tenant(&tenant.id).unwrap().unwrap();
        assert_eq!(updated.max_descriptors, 500);
        assert_eq!(updated.max_storage_bytes, 5_000_000);
        assert_eq!(updated.mu_limit, 50_000);
    }

    #[test]
    fn tenant_usage() {
        let dir = tempfile::tempdir().unwrap();
        let tm = TenantManager::open(&dir.path().join("tenants.db")).unwrap();

        let tenant = tm.create_tenant("test-org", "starter").unwrap();
        let usage = tm.get_usage(&tenant.id).unwrap();
        assert_eq!(usage.current_descriptors, 0);
        assert_eq!(usage.current_bytes, 0);
        assert_eq!(usage.mu_balance, 100_000);
        assert_eq!(usage.mu_limit, 100_000);
    }
}
