//! `mesh-hub` — high-capacity mesh node with disk-backed storage, multi-tenant
//! management, and admin API.
//!
//! A hub is a mesh node that speaks the same protocol as any node but operates
//! at a higher tier: disk-backed storage, full routing tables, multi-tenant
//! commercial service, and an admin API.

pub mod admin;
pub mod auth;
pub mod config;
pub mod hooks;
pub mod metrics;
pub mod network;
pub mod peering;
pub mod policy;
pub mod rate_limit;
pub mod seeding;
pub mod storage;
pub mod tenant;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use mesh_core::frame::{
    MSG_FIND_NODE, MSG_FIND_NODE_RESULT, MSG_FIND_VALUE, MSG_FIND_VALUE_RESULT, MSG_PING, MSG_PONG,
    MSG_STORE, MSG_STORE_ACK,
};
use mesh_core::identity::{Identity, Keypair};
use mesh_core::message::{FindNode, FindValue, NodeAddr, Ping, Store, from_cbor, to_cbor};
use mesh_core::Frame;
use mesh_dht::{DescriptorStorage, DhtConfig, DhtNode};
use mesh_transport::connection::ResponseSender;
use mesh_transport::endpoint::MeshEndpoint;

use crate::admin::{AdminState, admin_router};
use crate::config::HubConfig;
use crate::hooks::HubProtocolHook;
use crate::metrics::HubMetrics;
use crate::peering::{HubMetadata, PeerManager, PeerState, PeerStatus};
use crate::policy::PolicyEngine;
use crate::rate_limit::HubRateLimiter;
use crate::storage::CachedStorage;
use crate::storage::redb::RedbStorage;
use crate::tenant::TenantManager;

/// Type alias for the hub's DhtNode with disk-backed cached storage.
pub type HubDhtNode = DhtNode<CachedStorage<RedbStorage>>;

/// The core hub runtime — owns all hub state and runs the event loops.
pub struct HubRuntime {
    config: HubConfig,
    hub_identity: Identity,
    dht_node: Arc<Mutex<HubDhtNode>>,
    endpoint: MeshEndpoint,
    tenant_manager: Arc<Mutex<TenantManager>>,
    /// Peer manager for hub-to-hub peering (None if peering is disabled).
    peer_manager: Option<Arc<tokio::sync::Mutex<PeerManager>>>,
    /// Rate limiter for per-IP and per-identity throttling.
    rate_limiter: Arc<HubRateLimiter>,
    /// Hub metrics (None if observability is disabled).
    metrics: Option<HubMetrics>,
    /// Hub keypair (copy for seeding).
    keypair: Keypair,
}

impl HubRuntime {
    /// Create a builder for configuring the hub.
    pub fn builder(config: HubConfig, keypair: Keypair) -> HubBuilder {
        HubBuilder { config, keypair }
    }

    /// The hub's admin API router. Downstream projects can merge additional routes.
    pub fn admin_router(&self) -> axum::Router {
        let state = Arc::new(AdminState {
            dht_node: self.dht_node.clone(),
            tenant_manager: self.tenant_manager.clone(),
            start_time: std::time::Instant::now(),
            hub_did: Some(self.hub_identity.did()),
            operator_token: self.config.operator_token.clone(),
            metrics: self.metrics.clone(),
        });
        admin_router(state)
    }

    /// Start the hub: admin API, expiry task, and protocol listener.
    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let admin_addr = self.config.network.admin_addr;
        let admin_state = Arc::new(AdminState {
            dht_node: self.dht_node.clone(),
            tenant_manager: self.tenant_manager.clone(),
            start_time: std::time::Instant::now(),
            hub_did: Some(self.hub_identity.did()),
            operator_token: self.config.operator_token.clone(),
            metrics: self.metrics.clone(),
        });
        let router = admin_router(admin_state);

        // Spawn admin API server
        let listener = tokio::net::TcpListener::bind(admin_addr).await?;
        tracing::info!(%admin_addr, "admin API listening");
        tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, router).await {
                tracing::error!("admin API error: {e}");
            }
        });

        // Spawn expiry background task (every 60s)
        let node_for_expiry = self.dht_node.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
                if let Ok(mut node) = node_for_expiry.lock() {
                    node.store.evict_expired();
                    tracing::debug!("expired descriptors evicted");
                }
            }
        });

        // Spawn rate limiter cleanup task (every 2 minutes)
        let rl = self.rate_limiter.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(120)).await;
                rl.cleanup();
                tracing::debug!("rate limiter stale entries cleaned");
            }
        });

        // Spawn peering background tasks (if enabled)
        if let Some(ref peer_manager) = self.peer_manager {
            let pm = peer_manager.clone();
            let dht = self.dht_node.clone();

            // Publish self-advertisement on startup
            peering::publish_self_advertisement(&dht, &pm);

            // Self-advertisement re-publish task (every TTL/2 = 1800s)
            let pm_adv = pm.clone();
            let dht_adv = dht.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(1800)).await;
                    peering::publish_self_advertisement(&dht_adv, &pm_adv);
                }
            });

            // Bootstrap from seed peers (breaks chicken-and-egg)
            let seed_peers = self.config.peering.seed_peers.clone();
            if !seed_peers.is_empty() {
                let pm_boot = pm.clone();
                let dht_boot = dht.clone();
                peering::bootstrap_from_seeds(&dht_boot, &pm_boot, &seed_peers).await;
            }

            // Discovery task: run on startup, then every 5 minutes
            let pm_disc = pm.clone();
            let dht_disc = dht.clone();
            tokio::spawn(async move {
                // Initial discovery (now has seed peer ads in local storage)
                peering::run_discovery(&dht_disc, &pm_disc).await;
                loop {
                    tokio::time::sleep(Duration::from_secs(300)).await;
                    peering::run_discovery(&dht_disc, &pm_disc).await;
                }
            });

            // Gossip task
            let gossip_interval = self.config.peering.gossip_interval_secs;
            let pm_gossip = pm.clone();
            let dht_gossip = dht.clone();
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(gossip_interval)).await;
                    peering::run_gossip_round(&dht_gossip, &pm_gossip).await;
                }
            });

            // Health check task
            let health_interval = self.config.peering.health_check_interval_secs;
            let pm_health = pm;
            tokio::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(health_interval)).await;
                    peering::run_health_check(&pm_health).await;
                }
            });

            tracing::info!("hub peering enabled");
        }

        // Spawn seeding background task (if enabled)
        if self.config.seeding.enabled {
            if let Some(ref seed_file) = self.config.seeding.seed_file {
                match seeding::load_seed_file(seed_file) {
                    Ok(entries) => {
                        // Seed immediately on startup
                        let (ok, fail) = seeding::seed_now(
                            &self.dht_node,
                            &self.keypair,
                            &entries,
                        );
                        tracing::info!(ok, fail, "initial seeding complete");

                        // Spawn periodic re-seeder
                        let interval = Duration::from_secs(self.config.seeding.interval_secs);
                        seeding::spawn_seeder(
                            self.dht_node.clone(),
                            Keypair::from_bytes(&self.keypair.secret_bytes()),
                            entries,
                            interval,
                        );
                        tracing::info!(
                            interval_secs = self.config.seeding.interval_secs,
                            "seeding background task started"
                        );
                    }
                    Err(e) => {
                        tracing::error!(error = %e, path = %seed_file.display(), "failed to load seed file");
                    }
                }
            }
        }

        tracing::info!(
            did = %self.hub_identity.did(),
            addr = %self.config.network.listen_addr,
            "mesh hub started"
        );

        // Protocol listener
        let dht_node = self.dht_node.clone();
        let hub_metrics = self.metrics.clone();
        let pm_for_listener = self.peer_manager.clone();
        let listen_future = self.endpoint.listen(move |frame, sender, peer_identity| {
            let dht_node = dht_node.clone();
            let hub_metrics = hub_metrics.clone();
            let pm = pm_for_listener.clone();
            async move {
                handle_protocol_request(frame, sender, peer_identity, dht_node, hub_metrics, pm)
                    .await;
            }
        });

        // Run until shutdown signal
        tokio::select! {
            result = listen_future => {
                if let Err(e) = result {
                    tracing::error!("listener error: {e}");
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("shutdown signal received");
            }
        }

        // Graceful shutdown
        self.endpoint.close();
        self.endpoint.wait_idle().await;
        tracing::info!("hub shutdown complete");

        Ok(())
    }
}

/// Builder for configuring and constructing a [`HubRuntime`].
pub struct HubBuilder {
    config: HubConfig,
    keypair: Keypair,
}

impl HubBuilder {
    /// Build the hub runtime.
    pub fn build(self) -> Result<HubRuntime, Box<dyn std::error::Error>> {
        // Ensure data directories exist
        std::fs::create_dir_all(&self.config.storage.data_dir)?;
        if let Some(parent) = self.config.tenants.db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // L2: redb storage
        let redb_path = self.config.storage.data_dir.join("descriptors.redb");
        let redb_storage = RedbStorage::open(&redb_path)?;

        // L1: hot cache wrapping L2
        let cached_storage =
            CachedStorage::new(redb_storage, self.config.storage.hot_cache_entries);

        // L3: tenant database
        let tenant_manager = Arc::new(Mutex::new(TenantManager::open(
            &self.config.tenants.db_path,
        )?));

        // Rate limiter
        let rate_limiter = Arc::new(HubRateLimiter::new(self.config.rate_limit.clone()));

        // Observability: initialize metrics if enabled
        let hub_metrics = if self.config.observability.metrics_enabled {
            Some(HubMetrics::new())
        } else {
            None
        };

        // Policy engine + protocol hook
        let policy = PolicyEngine::new(self.config.policy.clone());
        let mut hook = HubProtocolHook::new(policy, tenant_manager.clone(), rate_limiter.clone());
        if let Some(ref m) = hub_metrics {
            hook = hook.with_metrics(m.clone());
        }
        let hook = Arc::new(hook);

        // QUIC endpoint (before moving keypair)
        let listen_addr = self.config.network.listen_addr;
        let endpoint = MeshEndpoint::new(listen_addr, &self.keypair)?;

        // DhtNode with cached storage + hook
        let hub_identity = self.keypair.identity();
        let node_addr = NodeAddr {
            protocol: "quic".into(),
            address: listen_addr.to_string(),
        };

        // Create PeerManager if peering is enabled (before moving keypair)
        let peer_manager = if self.config.peering.enabled {
            let peering_keypair = Keypair::from_bytes(&self.keypair.secret_bytes());
            let peering_endpoint =
                MeshEndpoint::new("0.0.0.0:0".parse().unwrap(), &peering_keypair)?;
            let hub_metadata = HubMetadata {
                max_descriptors: self.config.peering.max_descriptors,
                regions: self.config.peering.regions.clone(),
                endpoint: format!("quic://{}", listen_addr),
            };
            Some(Arc::new(tokio::sync::Mutex::new(PeerManager::new(
                hub_identity.clone(),
                peering_keypair,
                node_addr.clone(),
                peering_endpoint,
                hub_metadata,
                self.config.peering.max_peers,
                self.config.security.outbound_allowlist.clone(),
            ))))
        } else {
            None
        };

        let seeding_keypair = Keypair::from_bytes(&self.keypair.secret_bytes());
        let dht_node =
            DhtNode::with_store(self.keypair, node_addr, DhtConfig::default(), cached_storage)
                .with_hooks(hook);
        let dht_node = Arc::new(Mutex::new(dht_node));

        Ok(HubRuntime {
            config: self.config,
            hub_identity,
            dht_node,
            endpoint,
            tenant_manager,
            peer_manager,
            rate_limiter,
            metrics: hub_metrics,
            keypair: seeding_keypair,
        })
    }
}

async fn handle_protocol_request(
    frame: Frame,
    sender: ResponseSender,
    peer_identity: Option<Identity>,
    dht_node: Arc<Mutex<HubDhtNode>>,
    hub_metrics: Option<HubMetrics>,
    peer_manager: Option<Arc<tokio::sync::Mutex<PeerManager>>>,
) {
    let start = std::time::Instant::now();
    let msg_type = frame.msg_type;
    let msg_id = frame.msg_id;

    // Pre-check: is the sender a known peer hub? (requires async lock, done before sync lock)
    // Also auto-register incoming connections from hub identities as peers.
    let is_peer_hub = if let (Some(pm), Some(pid)) = (&peer_manager, &peer_identity) {
        let mut pm = pm.lock().await;
        if pm.peers.get(&pid.did()).is_some_and(|p| p.status == PeerStatus::Connected) {
            true
        } else {
            // Check if this identity has a hub advertisement in our storage —
            // if so, they're a peer hub connecting to us. Register them.
            let has_hub_ad = {
                let node = dht_node.lock().unwrap();
                let hub_descs = node.store.get_descriptors(
                    &mesh_schemas::ROUTING_KEY_INFRASTRUCTURE_HUB,
                    None,
                );
                hub_descs.iter().any(|d| d.publisher.did() == pid.did())
            };
            if has_hub_ad {
                let did = pid.did();
                if !pm.peers.contains_key(&did) {
                    tracing::info!(%did, "auto-registered incoming peer hub");
                    pm.peers.insert(did, PeerState {
                        connection: None, // incoming — we don't own the connection
                        addr: NodeAddr { protocol: "quic".into(), address: "incoming".into() },
                        status: PeerStatus::Connected,
                        last_seen: std::time::Instant::now(),
                        consecutive_failures: 0,
                    });
                }
                true
            } else {
                false
            }
        }
    } else {
        false
    };

    let response = {
        let mut node = dht_node.lock().unwrap();

        match msg_type {
            MSG_PING => {
                let Ok(ping) = from_cbor::<Ping>(&frame.body) else {
                    return;
                };
                if let Err(reason) = mesh_dht::verify_sender_binding(&ping.sender, &peer_identity) {
                    tracing::warn!(reason, "rejecting PING");
                    return;
                }
                let pong = node.handle_ping(&ping);
                let body = to_cbor(&pong).unwrap();
                Frame::response(&frame, MSG_PONG, body)
            }
            MSG_STORE => {
                let Ok(store_req) = from_cbor::<Store>(&frame.body) else {
                    return;
                };
                if let Err(reason) = mesh_dht::verify_sender_binding(&store_req.sender, &peer_identity) {
                    tracing::warn!(reason, "rejecting STORE");
                    return;
                }

                let ack = if is_peer_hub {
                    // Peer hubs bypass hooks and rate limits — gossip must not be throttled.
                    // Write directly to storage, skipping pre_store policy checks.
                    node.store.inner_mut().skip_rate_limit = true;
                    let result = node.store.store_descriptor(store_req.descriptor.clone());
                    node.store.inner_mut().skip_rate_limit = false;
                    mesh_core::message::StoreAck {
                        stored: result.is_ok(),
                        reason: result.err().map(|e| e.to_string()),
                    }
                } else {
                    node.handle_store(&store_req)
                };

                let body = to_cbor(&ack).unwrap();
                Frame::response(&frame, MSG_STORE_ACK, body)
            }
            MSG_FIND_NODE => {
                let Ok(find) = from_cbor::<FindNode>(&frame.body) else {
                    return;
                };
                if let Err(reason) = mesh_dht::verify_sender_binding(&find.sender, &peer_identity) {
                    tracing::warn!(reason, "rejecting FIND_NODE");
                    return;
                }
                let result = node.handle_find_node(&find);
                let body = to_cbor(&result).unwrap();
                Frame::response(&frame, MSG_FIND_NODE_RESULT, body)
            }
            MSG_FIND_VALUE => {
                let Ok(find) = from_cbor::<FindValue>(&frame.body) else {
                    return;
                };
                if let Err(reason) = mesh_dht::verify_sender_binding(&find.sender, &peer_identity) {
                    tracing::warn!(reason, "rejecting FIND_VALUE");
                    return;
                }
                let result = node.handle_find_value(&find);
                let desc_count = result.descriptors.as_ref().map_or(0, |d| d.len());
                let node_count = result.nodes.as_ref().map_or(0, |n| n.len());
                tracing::info!(key = %find.key, desc_count, node_count, "FIND_VALUE handled");
                let body = to_cbor(&result).unwrap();
                Frame::response(&frame, MSG_FIND_VALUE_RESULT, body)
            }
            _ => {
                tracing::debug!(msg_type, "unknown message type");
                return;
            }
        }
    };

    // Record latency metrics (aggregate, no per-tenant labels).
    let elapsed = start.elapsed().as_secs_f64();
    if let Some(ref m) = hub_metrics {
        match msg_type {
            MSG_STORE => m.record_store(elapsed),
            MSG_FIND_VALUE => m.record_query(elapsed),
            _ => {}
        }
    }

    tracing::trace!(msg_type, msg_id = %hex::encode(msg_id), elapsed_ms = elapsed * 1000.0, "request handled");

    if let Err(e) = sender.send(&response).await {
        tracing::debug!("failed to send response: {e}");
    }
}
