//! Hub configuration (TOML deserialization).

use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;

/// Top-level hub configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct HubConfig {
    pub identity: IdentityConfig,
    pub network: NetworkConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub tenants: TenantConfig,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub peering: PeeringConfig,
}

/// Hub-to-hub peering configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct PeeringConfig {
    /// Enable hub peering (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Interval in seconds between gossip rounds (default: 60).
    #[serde(default = "default_gossip_interval")]
    pub gossip_interval_secs: u64,
    /// Interval in seconds between health checks (default: 10).
    #[serde(default = "default_health_check_interval")]
    pub health_check_interval_secs: u64,
    /// Maximum number of connected peers (default: 50).
    #[serde(default = "default_max_peers")]
    pub max_peers: usize,
    /// Regions this hub serves (for metadata advertisement).
    #[serde(default)]
    pub regions: Vec<String>,
    /// Maximum descriptors this hub stores (for metadata advertisement).
    #[serde(default = "default_max_descriptors")]
    pub max_descriptors: u64,
}

impl Default for PeeringConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            gossip_interval_secs: default_gossip_interval(),
            health_check_interval_secs: default_health_check_interval(),
            max_peers: default_max_peers(),
            regions: Vec::new(),
            max_descriptors: default_max_descriptors(),
        }
    }
}

fn default_gossip_interval() -> u64 {
    60
}

fn default_health_check_interval() -> u64 {
    10
}

fn default_max_peers() -> usize {
    50
}

fn default_max_descriptors() -> u64 {
    1_000_000
}

#[derive(Debug, Clone, Deserialize)]
pub struct IdentityConfig {
    pub keypair_path: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: SocketAddr,
    #[serde(default = "default_admin_addr")]
    pub admin_addr: SocketAddr,
}

fn default_listen_addr() -> SocketAddr {
    "0.0.0.0:4433".parse().unwrap()
}

fn default_admin_addr() -> SocketAddr {
    "127.0.0.1:8080".parse().unwrap()
}

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default = "default_hot_cache_entries")]
    pub hot_cache_entries: u64,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            data_dir: default_data_dir(),
            hot_cache_entries: default_hot_cache_entries(),
        }
    }
}

fn default_data_dir() -> PathBuf {
    PathBuf::from("data")
}

fn default_hot_cache_entries() -> u64 {
    10_000
}

#[derive(Debug, Clone, Deserialize)]
pub struct TenantConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_tenant_db")]
    pub db_path: PathBuf,
}

impl Default for TenantConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            db_path: default_tenant_db(),
        }
    }
}

fn default_tenant_db() -> PathBuf {
    PathBuf::from("data/tenants.sqlite")
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct PolicyConfig {
    #[serde(default)]
    pub store_mode: StoreMode,
    #[serde(default)]
    pub blocked_identities: Vec<String>,
    #[serde(default)]
    pub blocked_routing_keys: Vec<String>,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            store_mode: StoreMode::Open,
            blocked_identities: Vec::new(),
            blocked_routing_keys: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum StoreMode {
    #[default]
    Open,
    TenantOnly,
    Allowlist,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecurityConfig {
    #[serde(default = "default_50")]
    pub max_connections_per_ip: u32,
    #[serde(default = "default_20")]
    pub max_queries_per_identity_per_sec: u32,
    #[serde(default)]
    pub admin_bearer_token: Option<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            max_connections_per_ip: 50,
            max_queries_per_identity_per_sec: 20,
            admin_bearer_token: None,
        }
    }
}

fn default_50() -> u32 {
    50
}

fn default_20() -> u32 {
    20
}

impl HubConfig {
    pub fn from_file(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }
}
