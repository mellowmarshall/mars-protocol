//! Multi-tenant management with SQLite backend (L3).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use uuid::Uuid;

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
    pub created_at: u64,
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
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tenant_identities (
                tenant_id TEXT NOT NULL,
                identity_bytes BLOB NOT NULL,
                did TEXT NOT NULL,
                registered_at INTEGER NOT NULL,
                PRIMARY KEY (tenant_id, identity_bytes),
                FOREIGN KEY (tenant_id) REFERENCES tenants(id)
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn create_tenant(&self, name: &str, tier: &str) -> Result<Tenant, rusqlite::Error> {
        let id = Uuid::new_v4();
        let now = now_micros();
        let (max_desc, max_bytes, max_qr, max_sr) = tier_quotas(tier);

        self.conn.execute(
            "INSERT INTO tenants (id, name, tier, max_descriptors, max_storage_bytes, \
             max_query_rate, max_store_rate, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                id.to_string(),
                name,
                tier,
                max_desc as i64,
                max_bytes as i64,
                max_qr,
                max_sr,
                now as i64
            ],
        )?;

        Ok(Tenant {
            id,
            name: name.into(),
            tier: tier.into(),
            max_descriptors: max_desc,
            max_storage_bytes: max_bytes,
            max_query_rate: max_qr,
            max_store_rate: max_sr,
            current_descriptors: 0,
            current_bytes: 0,
            created_at: now,
        })
    }

    pub fn get_tenant(&self, id: &Uuid) -> Result<Option<Tenant>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, tier, max_descriptors, max_storage_bytes, max_query_rate, \
             max_store_rate, current_descriptors, current_bytes, created_at \
             FROM tenants WHERE id = ?1",
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
             max_store_rate, current_descriptors, current_bytes, created_at FROM tenants",
        )?;
        let rows = stmt.query_map([], |row| row_to_tenant(row))?;
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
}

fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

fn tier_quotas(tier: &str) -> (u64, u64, u32, u32) {
    match tier {
        "pro" => (10_000, 104_857_600, 100, 10),
        "enterprise" => (1_000_000, 10_737_418_240, 1000, 100),
        _ => (100, 1_048_576, 10, 1),
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
        created_at: row.get::<_, i64>(9)? as u64,
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

        // Get
        let loaded = tm.get_tenant(&tenant.id).unwrap().unwrap();
        assert_eq!(loaded.id, tenant.id);

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
}
