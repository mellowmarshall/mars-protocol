//! Hub-to-hub peering — discovery, connection management, gossip, and health checks.
//!
//! The `PeerManager` handles all peer-to-peer hub interactions using the same
//! wire protocol messages (PING, PONG, STORE, FIND_VALUE) that regular nodes use.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use mesh_core::descriptor::Descriptor;
use mesh_core::frame::{MSG_FIND_VALUE, MSG_FIND_VALUE_RESULT, MSG_PING, MSG_PONG, MSG_STORE};
use mesh_core::identity::{Identity, Keypair};
use mesh_core::message::{FindValue, FindValueResult, NodeAddr, Ping, Pong, Store, from_cbor, to_cbor};
use mesh_core::Frame;
use mesh_dht::DescriptorStorage;
use mesh_schemas::{
    ROUTING_KEY_INFRASTRUCTURE, ROUTING_KEY_INFRASTRUCTURE_HUB, SCHEMA_HASH_INFRA_HUB,
};
use mesh_transport::endpoint::MeshEndpoint;
use mesh_transport::send_request;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::network::validate_outbound_addr;
use crate::HubDhtNode;

/// Maximum descriptors per gossip round per peer.
const MAX_GOSSIP_BATCH: usize = 50;

/// Consecutive health-check failures before marking a peer unhealthy.
const UNHEALTHY_THRESHOLD: u32 = 3;

/// Consecutive health-check failures before disconnecting a peer.
const DISCONNECT_THRESHOLD: u32 = 5;

/// Hub metadata included in the self-advertisement descriptor payload.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HubMetadata {
    /// Maximum number of descriptors this hub stores.
    pub max_descriptors: u64,
    /// Regions this hub serves.
    pub regions: Vec<String>,
    /// QUIC endpoint address (e.g., "quic://hub.example.com:4433").
    pub endpoint: String,
}

/// Status of a peer hub connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PeerStatus {
    /// Connected and healthy.
    Connected,
    /// Disconnected (not currently reachable).
    Disconnected,
    /// Connected but failing health checks.
    Unhealthy,
}

/// State for a single peer hub.
pub struct PeerState {
    /// Active QUIC connection (if connected).
    pub connection: Option<mesh_transport::MeshConnection>,
    /// The peer's network address.
    pub addr: NodeAddr,
    /// Current connection status.
    pub status: PeerStatus,
    /// Last time we successfully heard from this peer.
    pub last_seen: Instant,
    /// Number of consecutive health-check failures.
    pub consecutive_failures: u32,
}

/// Manages hub-to-hub peering: discovery, connections, gossip, and health checks.
pub struct PeerManager {
    /// Our hub's identity.
    pub(crate) hub_identity: Identity,
    /// Our hub's keypair for signing.
    hub_keypair: Keypair,
    /// Our hub's advertised address.
    pub(crate) hub_addr: NodeAddr,
    /// Connected peer hubs, keyed by DID.
    pub(crate) peers: HashMap<String, PeerState>,
    /// Our QUIC endpoint for outgoing connections.
    endpoint: MeshEndpoint,
    /// Hub metadata for self-advertisement.
    hub_metadata: HubMetadata,
    /// Maximum number of connected peers.
    max_peers: usize,
    /// Addresses allowed for outbound connections even if private/loopback (SSRF allowlist).
    outbound_allowlist: Vec<String>,
}

impl PeerManager {
    /// Create a new PeerManager.
    pub fn new(
        hub_identity: Identity,
        hub_keypair: Keypair,
        hub_addr: NodeAddr,
        endpoint: MeshEndpoint,
        hub_metadata: HubMetadata,
        max_peers: usize,
        outbound_allowlist: Vec<String>,
    ) -> Self {
        Self {
            hub_identity,
            hub_keypair,
            hub_addr,
            peers: HashMap::new(),
            endpoint,
            hub_metadata,
            max_peers,
            outbound_allowlist,
        }
    }

    /// Create a signed self-advertisement descriptor for this hub.
    ///
    /// The descriptor advertises this hub on the `infrastructure/hub` routing key
    /// so other hubs can discover it via FIND_VALUE.
    pub fn self_advertisement_descriptor(&self) -> Result<Descriptor, Box<dyn std::error::Error>> {
        let payload = to_cbor(&self.hub_metadata)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let descriptor = Descriptor::create(
            &self.hub_keypair,
            SCHEMA_HASH_INFRA_HUB.clone(),
            "hub".into(),
            payload,
            now,
            now / 1_000_000, // sequence derived from timestamp for monotonicity
            3600,            // 1 hour TTL
            vec![
                ROUTING_KEY_INFRASTRUCTURE.clone(),
                ROUTING_KEY_INFRASTRUCTURE_HUB.clone(),
            ],
        )?;

        Ok(descriptor)
    }

    /// Discover peer hubs from a set of descriptors.
    ///
    /// Parses hub metadata from each descriptor's payload, filters out self,
    /// and returns (Identity, NodeAddr, HubMetadata) tuples.
    pub fn discover_peers_from_descriptors(
        &self,
        descriptors: &[Descriptor],
    ) -> Vec<(Identity, NodeAddr, HubMetadata)> {
        let our_did = self.hub_identity.did();
        let mut discovered = Vec::new();

        for desc in descriptors {
            // Skip our own advertisement
            if desc.publisher.did() == our_did {
                continue;
            }

            // Parse the hub metadata from the payload
            match from_cbor::<HubMetadata>(&desc.payload) {
                Ok(metadata) => {
                    let addr = endpoint_to_node_addr(&metadata.endpoint);
                    discovered.push((desc.publisher.clone(), addr, metadata));
                }
                Err(e) => {
                    tracing::warn!(
                        publisher = %desc.publisher.did(),
                        error = %e,
                        "failed to parse hub metadata from descriptor"
                    );
                }
            }
        }

        discovered
    }

    /// Connect to a peer hub at the given address.
    ///
    /// Stores the connection in the peers map on success.
    pub async fn connect_to_peer(
        &mut self,
        identity: &Identity,
        addr: &NodeAddr,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let did = identity.did();

        // Don't exceed max peers
        let connected_count = self
            .peers
            .values()
            .filter(|p| p.status == PeerStatus::Connected)
            .count();
        if connected_count >= self.max_peers {
            tracing::debug!(max = self.max_peers, "max peers reached, skipping connect");
            return Ok(());
        }

        // Don't reconnect to already-connected peers
        if let Some(peer) = self.peers.get(&did)
            && peer.status == PeerStatus::Connected
            && peer.connection.is_some()
        {
            tracing::debug!(%did, "already connected to peer");
            return Ok(());
        }

        // SSRF prevention: validate that the outbound address is not private/loopback/reserved
        let socket_addr = validate_outbound_addr(&addr.address, &self.outbound_allowlist)
            .map_err(|e| {
                tracing::warn!(%did, addr = %addr.address, error = %e, "outbound address rejected by SSRF filter");
                format!("outbound address rejected: {}", e)
            })?;

        match self.endpoint.connect(socket_addr).await {
            Ok(connection) => {
                tracing::info!(%did, addr = %addr.address, "connected to peer hub");
                self.peers.insert(
                    did,
                    PeerState {
                        connection: Some(connection),
                        addr: addr.clone(),
                        status: PeerStatus::Connected,
                        last_seen: Instant::now(),
                        consecutive_failures: 0,
                    },
                );
                Ok(())
            }
            Err(e) => {
                tracing::warn!(%did, addr = %addr.address, error = %e, "failed to connect to peer hub");
                self.peers
                    .entry(did)
                    .and_modify(|p| {
                        p.status = PeerStatus::Disconnected;
                        p.consecutive_failures += 1;
                    })
                    .or_insert(PeerState {
                        connection: None,
                        addr: addr.clone(),
                        status: PeerStatus::Disconnected,
                        last_seen: Instant::now(),
                        consecutive_failures: 1,
                    });
                Err(e.into())
            }
        }
    }

    /// Send a batch of descriptors to all connected peers via STORE messages.
    ///
    /// Sends up to `MAX_GOSSIP_BATCH` descriptors per peer per round.
    pub async fn gossip_round(&mut self, descriptors: Vec<Descriptor>) {
        let batch: Vec<&Descriptor> = descriptors.iter().take(MAX_GOSSIP_BATCH).collect();
        if batch.is_empty() {
            return;
        }

        let dids: Vec<String> = self
            .peers
            .iter()
            .filter(|(_, p)| p.status == PeerStatus::Connected && p.connection.is_some())
            .map(|(did, _)| did.clone())
            .collect();

        for did in dids {
            let mut failures = 0u32;
            for desc in &batch {
                let store_msg = Store {
                    sender: self.hub_identity.clone(),
                    sender_addr: self.hub_addr.clone(),
                    descriptor: (*desc).clone(),
                };

                let body = match to_cbor(&store_msg) {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::warn!(error = %e, "failed to encode STORE for gossip");
                        continue;
                    }
                };

                let frame = Frame::new(MSG_STORE, body);

                let result = if let Some(peer) = self.peers.get(&did) {
                    if let Some(ref conn) = peer.connection {
                        Some(send_request(conn, &frame).await)
                    } else {
                        None
                    }
                } else {
                    None
                };

                match result {
                    Some(Ok(_response)) => {
                        tracing::trace!(peer = %did, "gossip STORE sent");
                    }
                    Some(Err(e)) => {
                        tracing::debug!(peer = %did, error = %e, "gossip STORE failed");
                        failures += 1;
                    }
                    None => {
                        failures += 1;
                    }
                }
            }

            if failures > 0
                && let Some(peer) = self.peers.get_mut(&did)
            {
                peer.consecutive_failures += failures;
                tracing::debug!(
                    peer = %did,
                    failures,
                    total_failures = peer.consecutive_failures,
                    "gossip round had failures"
                );
            }
        }
    }

    /// Perform a health check on all connected peers via PING/PONG.
    ///
    /// Updates peer status based on responses:
    /// - Successful PONG: reset failures, mark Connected, update last_seen
    /// - Timeout/error: increment failures
    /// - >= UNHEALTHY_THRESHOLD failures: mark Unhealthy
    /// - >= DISCONNECT_THRESHOLD failures: disconnect
    pub async fn health_check(&mut self) {
        let dids: Vec<String> = self
            .peers
            .iter()
            .filter(|(_, p)| p.status != PeerStatus::Disconnected)
            .map(|(did, _)| did.clone())
            .collect();

        for did in dids {
            let ping = Ping {
                sender: self.hub_identity.clone(),
                sender_addr: self.hub_addr.clone(),
            };

            let body = match to_cbor(&ping) {
                Ok(b) => b,
                Err(_) => continue,
            };

            let frame = Frame::new(MSG_PING, body);

            let result = if let Some(peer) = self.peers.get(&did) {
                if let Some(ref conn) = peer.connection {
                    Some(send_request(conn, &frame).await)
                } else {
                    None
                }
            } else {
                None
            };

            match result {
                Some(Ok(response)) if response.msg_type == MSG_PONG => {
                    if from_cbor::<Pong>(&response.body).is_ok()
                        && let Some(peer) = self.peers.get_mut(&did)
                    {
                        peer.consecutive_failures = 0;
                        peer.status = PeerStatus::Connected;
                        peer.last_seen = Instant::now();
                        tracing::trace!(peer = %did, "health check OK");
                    }
                }
                _ => {
                    if let Some(peer) = self.peers.get_mut(&did) {
                        peer.consecutive_failures += 1;
                        let failures = peer.consecutive_failures;

                        if failures >= DISCONNECT_THRESHOLD {
                            tracing::warn!(
                                peer = %did,
                                failures,
                                "peer exceeded disconnect threshold, disconnecting"
                            );
                            peer.status = PeerStatus::Disconnected;
                            peer.connection = None;
                        } else if failures >= UNHEALTHY_THRESHOLD {
                            tracing::info!(
                                peer = %did,
                                failures,
                                "peer marked unhealthy"
                            );
                            peer.status = PeerStatus::Unhealthy;
                        }
                    }
                }
            }
        }
    }

    /// Get the number of connected peers.
    pub fn connected_peer_count(&self) -> usize {
        self.peers
            .values()
            .filter(|p| p.status == PeerStatus::Connected)
            .count()
    }

    /// Get a snapshot of all peer states (for admin API / diagnostics).
    pub fn peer_statuses(&self) -> Vec<(String, PeerStatus, u32)> {
        self.peers
            .iter()
            .map(|(did, state)| (did.clone(), state.status.clone(), state.consecutive_failures))
            .collect()
    }
}

/// Parse a hub endpoint string (e.g., "quic://host:port" or "host:port") into a NodeAddr.
fn endpoint_to_node_addr(endpoint: &str) -> NodeAddr {
    let address = endpoint
        .strip_prefix("quic://")
        .unwrap_or(endpoint)
        .to_string();
    NodeAddr {
        protocol: "quic".into(),
        address,
    }
}

// ── Background task functions ──
// These are spawned by HubRuntime::run() when peering is enabled.

/// Publish our self-advertisement descriptor to the local DHT.
pub fn publish_self_advertisement(
    dht_node: &Arc<StdMutex<HubDhtNode>>,
    peer_manager: &Arc<Mutex<PeerManager>>,
) {
    // We use try_lock since this is called from sync context; the tokio Mutex
    // try_lock doesn't require an async runtime.
    let pm = match peer_manager.try_lock() {
        Ok(pm) => pm,
        Err(_) => {
            tracing::debug!("peer_manager locked, skipping self-advertisement");
            return;
        }
    };

    let descriptor = match pm.self_advertisement_descriptor() {
        Ok(d) => d,
        Err(e) => {
            tracing::error!(error = %e, "failed to create self-advertisement descriptor");
            return;
        }
    };

    let store_msg = Store {
        sender: pm.hub_identity.clone(),
        sender_addr: pm.hub_addr.clone(),
        descriptor,
    };
    drop(pm);

    let mut node = dht_node.lock().unwrap();
    let _ack = node.handle_store(&store_msg);
    tracing::info!("published self-advertisement descriptor");
}

/// Discover peer hubs from the local DHT and connect to them.
pub async fn run_discovery(
    dht_node: &Arc<StdMutex<HubDhtNode>>,
    peer_manager: &Arc<Mutex<PeerManager>>,
) {
    let routing_key = ROUTING_KEY_INFRASTRUCTURE_HUB.clone();

    // Collect descriptors from DHT (short-lived std::sync lock)
    let descriptors = {
        let node = dht_node.lock().unwrap();
        node.store.get_descriptors(&routing_key, None)
    };

    if descriptors.is_empty() {
        tracing::debug!("no hub descriptors found for peer discovery");
        return;
    }

    // Parse discovered peers (short-lived tokio lock)
    let discovered = {
        let pm = peer_manager.lock().await;
        pm.discover_peers_from_descriptors(&descriptors)
    };

    tracing::info!(count = discovered.len(), "discovered peer hubs");

    // Connect to each discovered peer
    for (identity, addr, _metadata) in discovered {
        let did = identity.did();

        // Check if we should connect (short lock, no await)
        let should_connect = {
            let pm = peer_manager.lock().await;
            pm.peers
                .get(&did)
                .is_none_or(|p| p.status == PeerStatus::Disconnected)
        };

        if should_connect {
            let mut pm = peer_manager.lock().await;
            if let Err(e) = pm.connect_to_peer(&identity, &addr).await {
                tracing::debug!(%did, error = %e, "failed to connect to discovered peer");
            }
        }
    }
}

/// Bootstrap peering by contacting seed peers directly.
///
/// Breaks the chicken-and-egg problem: hubs can't discover peers via
/// local DHT because peer advertisements only arrive via gossip, which
/// requires an existing connection. Seed peers are contacted directly
/// via PING (to verify liveness) and FIND_VALUE (to fetch their hub
/// advertisements and any other hub ads they know about).
pub async fn bootstrap_from_seeds(
    dht_node: &Arc<StdMutex<HubDhtNode>>,
    peer_manager: &Arc<Mutex<PeerManager>>,
    seed_addrs: &[String],
) {
    if seed_addrs.is_empty() {
        return;
    }

    tracing::info!(count = seed_addrs.len(), "bootstrapping peering from seed peers");

    let pm = peer_manager.lock().await;
    let our_identity = pm.hub_identity.clone();
    let our_addr = pm.hub_addr.clone();
    drop(pm);

    for seed_addr_str in seed_addrs {
        let seed_addr = NodeAddr {
            protocol: "quic".into(),
            address: seed_addr_str.clone(),
        };

        // Step 1: PING the seed to verify it's alive and learn its identity
        let ping = Ping {
            sender: our_identity.clone(),
            sender_addr: our_addr.clone(),
        };
        let ping_body = match to_cbor(&ping) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let ping_frame = Frame::new(MSG_PING, ping_body);

        // We need a temporary connection. Use PeerManager's endpoint.
        let mut pm = peer_manager.lock().await;
        let socket_addr = match validate_outbound_addr(seed_addr_str, &pm.outbound_allowlist) {
            Ok(addr) => addr,
            Err(e) => {
                tracing::warn!(addr = %seed_addr_str, error = %e, "seed peer blocked by SSRF");
                continue;
            }
        };

        let connection = match pm.endpoint.connect(socket_addr).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(addr = %seed_addr_str, error = %e, "failed to connect to seed peer");
                continue;
            }
        };

        // Send PING
        let peer_identity = match send_request(&connection, &ping_frame).await {
            Ok(resp) if resp.msg_type == MSG_PONG => {
                match from_cbor::<Pong>(&resp.body) {
                    Ok(pong) => {
                        tracing::info!(
                            peer_did = %pong.sender.did(),
                            addr = %seed_addr_str,
                            "seed peer alive"
                        );
                        Some(pong.sender)
                    }
                    Err(_) => None,
                }
            }
            _ => {
                tracing::warn!(addr = %seed_addr_str, "seed peer did not respond to PING");
                None
            }
        };

        let peer_id = match peer_identity {
            Some(id) => id,
            None => continue,
        };

        // Step 2: Send FIND_VALUE for infrastructure/hub to get hub advertisements
        let find = FindValue {
            sender: our_identity.clone(),
            sender_addr: our_addr.clone(),
            key: ROUTING_KEY_INFRASTRUCTURE_HUB.clone(),
            max_results: 50,
            filters: None,
        };
        let find_body = match to_cbor(&find) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let find_frame = Frame::new(MSG_FIND_VALUE, find_body);

        if let Ok(resp) = send_request(&connection, &find_frame).await {
            if resp.msg_type == MSG_FIND_VALUE_RESULT {
                if let Ok(result) = from_cbor::<FindValueResult>(&resp.body) {
                    if let Some(descriptors) = result.descriptors {
                        // Store the hub advertisements in our local DHT
                        let mut node = dht_node.lock().unwrap();
                        node.store.inner_mut().skip_rate_limit = true;
                        let mut stored = 0;
                        for desc in &descriptors {
                            if node.store.store_descriptor(desc.clone()).is_ok() {
                                stored += 1;
                            }
                        }
                        node.store.inner_mut().skip_rate_limit = false;
                        tracing::info!(
                            stored,
                            total = descriptors.len(),
                            addr = %seed_addr_str,
                            "stored hub advertisements from seed peer"
                        );
                    }
                }
            }
        }

        // Step 3: Register the connection as a peer
        let did = peer_id.did();
        pm.peers.insert(
            did.clone(),
            PeerState {
                connection: Some(connection),
                addr: seed_addr.clone(),
                status: PeerStatus::Connected,
                last_seen: Instant::now(),
                consecutive_failures: 0,
            },
        );
        tracing::info!(%did, addr = %seed_addr_str, "seed peer registered");
    }
}

/// Collect ALL stored descriptors for gossip and send to peers.
///
/// This replicates the full descriptor set across hubs, ensuring every
/// hub converges to the same catalog. Deduplication is handled by the
/// receiver via sequence numbers (publisher + schema + topic).
pub async fn run_gossip_round(
    dht_node: &Arc<StdMutex<HubDhtNode>>,
    peer_manager: &Arc<Mutex<PeerManager>>,
) {
    // Collect all non-expired descriptors (short-lived std::sync lock)
    let descriptors = {
        let node = dht_node.lock().unwrap();
        node.store.inner().all_descriptors()
    };

    if descriptors.is_empty() {
        return;
    }

    tracing::debug!(count = descriptors.len(), "gossip round: sending descriptors to peers");

    let mut pm = peer_manager.lock().await;
    pm.gossip_round(descriptors).await;
}

/// Run health checks on all connected peers.
pub async fn run_health_check(peer_manager: &Arc<Mutex<PeerManager>>) {
    let mut pm = peer_manager.lock().await;
    pm.health_check().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::identity::Keypair;
    use mesh_core::message::NodeAddr;

    /// Create a test PeerManager. Requires a tokio runtime since MeshEndpoint
    /// binds a UDP socket.
    fn make_test_peer_manager() -> PeerManager {
        let keypair = Keypair::generate();
        let identity = keypair.identity();
        let addr = NodeAddr {
            protocol: "quic".into(),
            address: "127.0.0.1:4433".into(),
        };

        let endpoint = MeshEndpoint::new("127.0.0.1:0".parse().unwrap(), &keypair).unwrap();

        let metadata = HubMetadata {
            max_descriptors: 1_000_000,
            regions: vec!["us-east-1".into()],
            endpoint: "quic://127.0.0.1:4433".into(),
        };

        PeerManager::new(identity, keypair, addr, endpoint, metadata, 50, Vec::new())
    }

    #[tokio::test]
    async fn test_self_advertisement_descriptor() {
        let pm = make_test_peer_manager();
        let desc = pm.self_advertisement_descriptor().unwrap();

        // Check schema hash
        assert_eq!(desc.schema_hash, *SCHEMA_HASH_INFRA_HUB);

        // Check topic
        assert_eq!(desc.topic, "hub");

        // Check routing keys
        assert_eq!(desc.routing_keys.len(), 2);
        assert_eq!(desc.routing_keys[0], *ROUTING_KEY_INFRASTRUCTURE);
        assert_eq!(desc.routing_keys[1], *ROUTING_KEY_INFRASTRUCTURE_HUB);

        // Check TTL
        assert_eq!(desc.ttl, 3600);

        // Check publisher matches our identity
        assert_eq!(desc.publisher, pm.hub_identity);

        // Check the descriptor is valid
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        assert!(desc.validate(now).is_ok());

        // Check payload decodes back to HubMetadata
        let decoded: HubMetadata = from_cbor(&desc.payload).unwrap();
        assert_eq!(decoded.max_descriptors, 1_000_000);
        assert_eq!(decoded.regions, vec!["us-east-1".to_string()]);
        assert_eq!(decoded.endpoint, "quic://127.0.0.1:4433");
    }

    #[test]
    fn test_hub_metadata_cbor_roundtrip() {
        let metadata = HubMetadata {
            max_descriptors: 500_000,
            regions: vec!["eu-west-1".into(), "us-east-1".into()],
            endpoint: "quic://hub.example.com:4433".into(),
        };

        let bytes = to_cbor(&metadata).unwrap();
        let decoded: HubMetadata = from_cbor(&bytes).unwrap();

        assert_eq!(decoded.max_descriptors, metadata.max_descriptors);
        assert_eq!(decoded.regions, metadata.regions);
        assert_eq!(decoded.endpoint, metadata.endpoint);
    }

    #[test]
    fn test_hub_metadata_cbor_empty_regions() {
        let metadata = HubMetadata {
            max_descriptors: 0,
            regions: vec![],
            endpoint: "quic://localhost:4433".into(),
        };

        let bytes = to_cbor(&metadata).unwrap();
        let decoded: HubMetadata = from_cbor(&bytes).unwrap();

        assert_eq!(decoded.max_descriptors, 0);
        assert!(decoded.regions.is_empty());
    }

    #[test]
    fn test_peer_state_transitions() {
        // Start Connected
        let mut state = PeerState {
            connection: None,
            addr: NodeAddr {
                protocol: "quic".into(),
                address: "10.0.0.1:4433".into(),
            },
            status: PeerStatus::Connected,
            last_seen: Instant::now(),
            consecutive_failures: 0,
        };

        assert_eq!(state.status, PeerStatus::Connected);

        // Simulate failures up to unhealthy threshold
        for _ in 0..UNHEALTHY_THRESHOLD {
            state.consecutive_failures += 1;
        }
        assert!(state.consecutive_failures >= UNHEALTHY_THRESHOLD);
        state.status = PeerStatus::Unhealthy;
        assert_eq!(state.status, PeerStatus::Unhealthy);

        // Continue failures up to disconnect threshold
        for _ in UNHEALTHY_THRESHOLD..DISCONNECT_THRESHOLD {
            state.consecutive_failures += 1;
        }
        assert!(state.consecutive_failures >= DISCONNECT_THRESHOLD);
        state.status = PeerStatus::Disconnected;
        state.connection = None;
        assert_eq!(state.status, PeerStatus::Disconnected);
        assert!(state.connection.is_none());
    }

    #[test]
    fn test_peer_state_recovery() {
        // A peer can recover from Unhealthy back to Connected
        let mut state = PeerState {
            connection: None,
            addr: NodeAddr {
                protocol: "quic".into(),
                address: "10.0.0.1:4433".into(),
            },
            status: PeerStatus::Unhealthy,
            last_seen: Instant::now(),
            consecutive_failures: 3,
        };

        // Simulate successful health check
        state.consecutive_failures = 0;
        state.status = PeerStatus::Connected;
        state.last_seen = Instant::now();

        assert_eq!(state.status, PeerStatus::Connected);
        assert_eq!(state.consecutive_failures, 0);
    }

    #[test]
    fn test_endpoint_to_node_addr() {
        let addr = endpoint_to_node_addr("quic://hub.example.com:4433");
        assert_eq!(addr.protocol, "quic");
        assert_eq!(addr.address, "hub.example.com:4433");

        let addr2 = endpoint_to_node_addr("10.0.0.1:4433");
        assert_eq!(addr2.protocol, "quic");
        assert_eq!(addr2.address, "10.0.0.1:4433");
    }

    #[tokio::test]
    async fn test_discover_peers_filters_self() {
        let pm = make_test_peer_manager();

        // Create a self-advertisement
        let self_desc = pm.self_advertisement_descriptor().unwrap();

        // Create a descriptor from a different hub
        let other_keypair = Keypair::generate();
        let other_metadata = HubMetadata {
            max_descriptors: 200_000,
            regions: vec!["ap-southeast-1".into()],
            endpoint: "quic://10.0.0.2:4433".into(),
        };
        let other_payload = to_cbor(&other_metadata).unwrap();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
        let other_desc = Descriptor::create(
            &other_keypair,
            SCHEMA_HASH_INFRA_HUB.clone(),
            "hub".into(),
            other_payload,
            now,
            1,
            3600,
            vec![
                ROUTING_KEY_INFRASTRUCTURE.clone(),
                ROUTING_KEY_INFRASTRUCTURE_HUB.clone(),
            ],
        )
        .unwrap();

        let discovered = pm.discover_peers_from_descriptors(&[self_desc, other_desc]);

        // Should only discover the other hub, not ourselves
        assert_eq!(discovered.len(), 1);
        assert_eq!(discovered[0].0, other_keypair.identity());
        assert_eq!(discovered[0].1.address, "10.0.0.2:4433");
        assert_eq!(discovered[0].2.max_descriptors, 200_000);
    }

    #[tokio::test]
    async fn test_connected_peer_count() {
        let mut pm = make_test_peer_manager();
        assert_eq!(pm.connected_peer_count(), 0);

        pm.peers.insert(
            "did:mesh:zpeer1".into(),
            PeerState {
                connection: None,
                addr: NodeAddr {
                    protocol: "quic".into(),
                    address: "10.0.0.1:4433".into(),
                },
                status: PeerStatus::Connected,
                last_seen: Instant::now(),
                consecutive_failures: 0,
            },
        );
        assert_eq!(pm.connected_peer_count(), 1);

        pm.peers.insert(
            "did:mesh:zpeer2".into(),
            PeerState {
                connection: None,
                addr: NodeAddr {
                    protocol: "quic".into(),
                    address: "10.0.0.2:4433".into(),
                },
                status: PeerStatus::Disconnected,
                last_seen: Instant::now(),
                consecutive_failures: 5,
            },
        );
        // Disconnected peer shouldn't count
        assert_eq!(pm.connected_peer_count(), 1);
    }

    #[tokio::test]
    async fn test_peer_statuses() {
        let mut pm = make_test_peer_manager();
        pm.peers.insert(
            "did:mesh:zpeer1".into(),
            PeerState {
                connection: None,
                addr: NodeAddr {
                    protocol: "quic".into(),
                    address: "10.0.0.1:4433".into(),
                },
                status: PeerStatus::Connected,
                last_seen: Instant::now(),
                consecutive_failures: 0,
            },
        );
        pm.peers.insert(
            "did:mesh:zpeer2".into(),
            PeerState {
                connection: None,
                addr: NodeAddr {
                    protocol: "quic".into(),
                    address: "10.0.0.2:4433".into(),
                },
                status: PeerStatus::Unhealthy,
                last_seen: Instant::now(),
                consecutive_failures: 3,
            },
        );

        let statuses = pm.peer_statuses();
        assert_eq!(statuses.len(), 2);
    }
}
