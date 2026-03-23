//! DHT node — ties together routing table, descriptor storage, and message handling.
//!
//! `DhtNode` is the main entry point for DHT operations: handling incoming
//! protocol messages and performing iterative lookups.

use std::sync::Arc;

use mesh_core::frame::{
    MSG_FIND_NODE, MSG_FIND_NODE_RESULT, MSG_FIND_VALUE, MSG_FIND_VALUE_RESULT,
};
use mesh_core::identity::{Identity, Keypair};
use mesh_core::message::{
    FindNode, FindNodeResult, FindValue, FindValueResult, NodeAddr, NodeInfo, Ping, Pong, Store,
    StoreAck, from_cbor, to_cbor,
};
use mesh_core::{Descriptor, Frame, Hash};

use crate::distance::distance_cmp;
use crate::hooks::ProtocolHook;
use crate::routing::{K, RoutingTable};
use crate::storage::{DescriptorStorage, DescriptorStore};
use crate::transport::{Transport, TransportError};

/// Configuration for a DHT node.
#[derive(Debug, Clone)]
/// Configuration for the Kademlia DHT node.
pub struct DhtConfig {
    /// Kademlia concurrency parameter (α) — parallel lookups per iteration.
    pub alpha: usize,
    /// Maximum descriptors to return for a FIND_VALUE response.
    pub max_find_value_results: u16,
}

impl Default for DhtConfig {
    fn default() -> Self {
        Self {
            alpha: 3,
            max_find_value_results: 20,
        }
    }
}

/// A DHT node: identity, routing table, descriptor storage, and config.
///
/// Generic over `S: DescriptorStorage` to allow pluggable storage backends.
/// Defaults to the in-memory [`DescriptorStore`].
pub struct DhtNode<S: DescriptorStorage = DescriptorStore> {
    /// This node's keypair.
    keypair: Keypair,
    /// This node's public identity.
    identity: Identity,
    /// This node's DHT ID (BLAKE3 of public key).
    node_id: Hash,
    /// This node's advertised address.
    addr: NodeAddr,
    /// Kademlia routing table.
    pub routing_table: RoutingTable,
    /// Descriptor storage.
    pub store: S,
    /// Configuration.
    pub config: DhtConfig,
    /// Optional protocol hooks for metering, access control, and auditing.
    hooks: Option<Arc<dyn ProtocolHook>>,
}

impl DhtNode {
    /// Create a new DHT node with the default in-memory storage backend.
    pub fn new(keypair: Keypair, addr: NodeAddr, config: DhtConfig) -> Self {
        Self::with_store(keypair, addr, config, DescriptorStore::new())
    }
}

impl<S: DescriptorStorage> DhtNode<S> {
    /// Create a new DHT node with a custom storage backend.
    pub fn with_store(keypair: Keypair, addr: NodeAddr, config: DhtConfig, store: S) -> Self {
        let identity = keypair.identity();
        let node_id = identity.node_id();
        let routing_table = RoutingTable::new(node_id.clone());
        Self {
            keypair,
            identity,
            node_id,
            addr,
            routing_table,
            store,
            config,
            hooks: None,
        }
    }

    /// Attach protocol hooks for metering, access control, or auditing.
    pub fn with_hooks(mut self, hooks: Arc<dyn ProtocolHook>) -> Self {
        self.hooks = Some(hooks);
        self
    }

    /// Get this node's keypair.
    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    /// Get this node's identity.
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// Get this node's DHT ID.
    pub fn node_id(&self) -> &Hash {
        &self.node_id
    }

    /// Get this node's address.
    pub fn addr(&self) -> &NodeAddr {
        &self.addr
    }

    /// Handle a PING request (Section 3.4).
    pub fn handle_ping(&mut self, ping: &Ping) -> Pong {
        // Update routing table with sender info
        self.routing_table.add_node(NodeInfo {
            identity: ping.sender.clone(),
            addr: ping.sender_addr.clone(),
            last_seen: Self::now_micros(),
        });

        Pong {
            sender: self.identity.clone(),
            sender_addr: self.addr.clone(),
            observed_addr: ping.sender_addr.clone(),
        }
    }

    /// Handle a STORE request (Section 3.5).
    ///
    /// If protocol hooks are installed, calls `pre_store` before storing and
    /// `post_store` after a successful store.
    pub fn handle_store(&mut self, store_req: &Store) -> StoreAck {
        self.update_routing_table(&store_req.sender, &store_req.sender_addr);

        // Pre-store hook
        if let Some(ref hooks) = self.hooks {
            if let Err(reason) = hooks.pre_store(&store_req.descriptor) {
                return StoreAck {
                    stored: false,
                    reason: Some(reason),
                };
            }
        }

        match self.store.store_descriptor(store_req.descriptor.clone()) {
            Ok(()) => {
                // Post-store hook
                if let Some(ref hooks) = self.hooks {
                    hooks.post_store(&store_req.descriptor);
                }
                StoreAck {
                    stored: true,
                    reason: None,
                }
            }
            Err(e) => StoreAck {
                stored: false,
                reason: Some(e.to_string()),
            },
        }
    }

    /// Handle a FIND_NODE request (Section 3.6).
    ///
    /// Returns up to K closest nodes to the target, including ourselves
    /// so that a bootstrapping peer can learn our identity.
    pub fn handle_find_node(&mut self, find_node: &FindNode) -> FindNodeResult {
        self.update_routing_table(&find_node.sender, &find_node.sender_addr);

        let mut nodes = self.routing_table.closest_nodes(&find_node.target, K);

        // Include ourselves in the result so the requester can add us to
        // their routing table (fixes bootstrap with a single seed).
        let self_info = NodeInfo {
            identity: self.identity.clone(),
            addr: self.addr.clone(),
            last_seen: Self::now_micros(),
        };
        // Only add self if not already present and we'd fit within K.
        if !nodes.iter().any(|n| n.identity == self.identity) && nodes.len() < K {
            nodes.push(self_info);
        }

        FindNodeResult { nodes }
    }

    /// Handle a FIND_VALUE request (Section 3.7).
    ///
    /// If protocol hooks are installed, calls `pre_query` before lookup and
    /// `post_query` after results are produced.
    pub fn handle_find_value(&mut self, find_value: &FindValue) -> FindValueResult {
        self.update_routing_table(&find_value.sender, &find_value.sender_addr);

        // Pre-query hook
        if let Some(ref hooks) = self.hooks {
            if let Err(_) = hooks.pre_query(&find_value.key) {
                return FindValueResult {
                    descriptors: None,
                    nodes: None,
                };
            }
        }

        let filters = find_value.filters.as_ref();
        let descriptors = self.store.get_descriptors(&find_value.key, filters);

        let result = if descriptors.is_empty() {
            // No descriptors — return closest nodes instead
            let nodes = self.routing_table.closest_nodes(&find_value.key, K);
            FindValueResult {
                descriptors: None,
                nodes: Some(nodes),
            }
        } else {
            // Return descriptors, capped at server-side policy (clamp attacker-supplied value)
            let max = (find_value.max_results as usize).min(self.config.max_find_value_results as usize);
            let truncated = if descriptors.len() > max {
                descriptors[..max].to_vec()
            } else {
                descriptors
            };
            FindValueResult {
                descriptors: Some(truncated),
                nodes: None,
            }
        };

        // Post-query hook
        if let Some(ref hooks) = self.hooks {
            let count = result.descriptors.as_ref().map_or(0, |d| d.len());
            hooks.post_query(&find_value.key, count);
        }

        result
    }

    /// Perform an iterative Kademlia lookup for descriptors at a target key.
    ///
    /// Phase 0 simplified: queries α closest known nodes, follows closer nodes,
    /// collects descriptors found along the way.
    pub async fn lookup_value<T: Transport>(
        &mut self,
        target_key: &Hash,
        transport: &T,
    ) -> Result<Vec<Descriptor>, TransportError> {
        let alpha = self.config.alpha;
        let mut queried: Vec<Hash> = Vec::new();
        let mut collected_descriptors: Vec<Descriptor> = Vec::new();

        // Start with the α closest known nodes
        let initial = self.routing_table.closest_nodes(target_key, alpha);
        let mut candidates: Vec<NodeInfo> = initial;

        loop {
            // Pick unqueried candidates
            let to_query: Vec<NodeInfo> = candidates
                .iter()
                .filter(|n| {
                    let nid = n.identity.node_id();
                    !queried.contains(&nid)
                })
                .take(alpha)
                .cloned()
                .collect();

            if to_query.is_empty() {
                break;
            }

            let mut new_nodes: Vec<NodeInfo> = Vec::new();

            for node in &to_query {
                let nid = node.identity.node_id();
                queried.push(nid);

                // Build FIND_VALUE request
                let find_value = FindValue {
                    sender: self.identity.clone(),
                    sender_addr: self.addr.clone(),
                    key: target_key.clone(),
                    max_results: self.config.max_find_value_results,
                    filters: None,
                };
                let body =
                    to_cbor(&find_value).map_err(|e| TransportError::FrameError(e.to_string()))?;
                let frame = Frame::new(MSG_FIND_VALUE, body);

                match transport.send_request(&node.addr, frame).await {
                    Ok(resp) => {
                        if resp.msg_type == MSG_FIND_VALUE_RESULT
                            && let Ok(result) = from_cbor::<FindValueResult>(&resp.body)
                        {
                            if let Some(descs) = result.descriptors {
                                collected_descriptors.extend(descs);
                            }
                            if let Some(nodes) = result.nodes {
                                // Add new nodes to routing table and candidates
                                for n in nodes {
                                    self.routing_table.add_node(n.clone());
                                    new_nodes.push(n);
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Node unreachable — skip
                        continue;
                    }
                }
            }

            if new_nodes.is_empty() && collected_descriptors.is_empty() {
                break;
            }

            // Merge new nodes into candidates, sorted by distance
            candidates.extend(new_nodes);
            candidates.sort_by(|a, b| {
                let id_a = a.identity.node_id();
                let id_b = b.identity.node_id();
                distance_cmp(target_key, &id_a, &id_b)
            });
            candidates.truncate(K); // Keep only K closest

            // If we already found descriptors, stop
            if !collected_descriptors.is_empty() {
                break;
            }
        }

        Ok(collected_descriptors)
    }

    /// Bootstrap this node by connecting to seed addresses.
    ///
    /// Connects to each seed, performs FIND_NODE for our own ID to populate
    /// the routing table (Section 6.3).
    pub async fn bootstrap<T: Transport>(
        &mut self,
        seeds: &[NodeAddr],
        transport: &T,
    ) -> Result<usize, TransportError> {
        let mut discovered = 0;

        for seed_addr in seeds {
            // Send FIND_NODE for our own ID
            let find_node = FindNode {
                sender: self.identity.clone(),
                sender_addr: self.addr.clone(),
                target: self.node_id.clone(),
            };
            let body =
                to_cbor(&find_node).map_err(|e| TransportError::FrameError(e.to_string()))?;
            let frame = Frame::new(MSG_FIND_NODE, body);

            match transport.send_request(seed_addr, frame).await {
                Ok(resp) => {
                    if resp.msg_type == MSG_FIND_NODE_RESULT
                        && let Ok(result) = from_cbor::<FindNodeResult>(&resp.body)
                    {
                        for node in result.nodes {
                            self.routing_table.add_node(node);
                            discovered += 1;
                        }
                    }
                }
                Err(_) => {
                    // Seed unreachable — try next
                    continue;
                }
            }
        }

        Ok(discovered)
    }

    /// Add or update a sender in the routing table using their identity and address.
    ///
    /// All request messages now carry `sender_addr`, so every incoming message
    /// contributes to routing table freshness — standard Kademlia behavior.
    fn update_routing_table(&mut self, sender: &Identity, sender_addr: &NodeAddr) {
        self.routing_table.add_node(NodeInfo {
            identity: sender.clone(),
            addr: sender_addr.clone(),
            last_seen: Self::now_micros(),
        });
    }

    fn now_micros() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::hash::schema_hash;
    use mesh_core::message::FilterSet;
    use mesh_core::routing::routing_key;
    use std::collections::HashMap;
    use std::sync::Mutex;

    fn make_keypair_and_addr(port: u16) -> (Keypair, NodeAddr) {
        let kp = Keypair::generate();
        let addr = NodeAddr {
            protocol: "quic".into(),
            address: format!("127.0.0.1:{port}"),
        };
        (kp, addr)
    }

    fn make_node(port: u16) -> DhtNode {
        let (kp, addr) = make_keypair_and_addr(port);
        DhtNode::new(kp, addr, DhtConfig::default())
    }

    fn now_micros() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64
    }

    // ── Mock Transport ──

    /// A mock transport that routes requests to local DhtNode instances.
    struct MockTransport {
        /// Maps address strings to DhtNode instances.
        nodes: Mutex<HashMap<String, DhtNode>>,
    }

    impl MockTransport {
        fn new() -> Self {
            Self {
                nodes: Mutex::new(HashMap::new()),
            }
        }

        fn register(&self, node: DhtNode) {
            let addr = node.addr().address.clone();
            self.nodes.lock().unwrap().insert(addr, node);
        }
    }

    impl Transport for MockTransport {
        async fn send_request(
            &self,
            addr: &NodeAddr,
            frame: Frame,
        ) -> Result<Frame, TransportError> {
            let mut nodes = self.nodes.lock().unwrap();
            let node = nodes
                .get_mut(&addr.address)
                .ok_or_else(|| TransportError::Unreachable(addr.address.clone()))?;

            match frame.msg_type {
                MSG_FIND_NODE => {
                    let req: FindNode = from_cbor(&frame.body)
                        .map_err(|e| TransportError::FrameError(e.to_string()))?;
                    let result = node.handle_find_node(&req);
                    let body =
                        to_cbor(&result).map_err(|e| TransportError::FrameError(e.to_string()))?;
                    Ok(Frame::response(&frame, MSG_FIND_NODE_RESULT, body))
                }
                MSG_FIND_VALUE => {
                    let req: FindValue = from_cbor(&frame.body)
                        .map_err(|e| TransportError::FrameError(e.to_string()))?;
                    let result = node.handle_find_value(&req);
                    let body =
                        to_cbor(&result).map_err(|e| TransportError::FrameError(e.to_string()))?;
                    Ok(Frame::response(&frame, MSG_FIND_VALUE_RESULT, body))
                }
                _ => Err(TransportError::FrameError(format!(
                    "unexpected msg_type: 0x{:02x}",
                    frame.msg_type
                ))),
            }
        }
    }

    // ── Tests ──

    #[test]
    fn handle_ping() {
        let mut node = make_node(4433);
        let sender_kp = Keypair::generate();
        let ping = Ping {
            sender: sender_kp.identity(),
            sender_addr: NodeAddr {
                protocol: "quic".into(),
                address: "10.0.0.1:4433".into(),
            },
        };

        let pong = node.handle_ping(&ping);
        assert_eq!(pong.sender, node.identity().clone());
        assert_eq!(pong.observed_addr, ping.sender_addr);

        // Sender should be in routing table
        assert_eq!(node.routing_table.len(), 1);
    }

    #[test]
    fn handle_store_valid() {
        let mut node = make_node(4433);
        let kp = Keypair::generate();
        let now = now_micros();
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

        let store = Store {
            sender: kp.identity(),
            sender_addr: NodeAddr {
                protocol: "quic".into(),
                address: "10.0.0.99:4433".into(),
            },
            descriptor: desc,
        };

        let ack = node.handle_store(&store);
        assert!(ack.stored);
        assert!(ack.reason.is_none());
    }

    #[test]
    fn handle_store_invalid() {
        let mut node = make_node(4433);
        let kp = Keypair::generate();
        let now = now_micros();
        let mut desc = Descriptor::create(
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
        // Corrupt the payload so validation fails
        desc.payload = b"tampered".to_vec();

        let store = Store {
            sender: kp.identity(),
            sender_addr: NodeAddr {
                protocol: "quic".into(),
                address: "10.0.0.99:4433".into(),
            },
            descriptor: desc,
        };

        let ack = node.handle_store(&store);
        assert!(!ack.stored);
        assert!(ack.reason.is_some());
    }

    #[test]
    fn handle_find_node() {
        let mut node = make_node(4433);

        // Add some nodes to the routing table
        for i in 0..5 {
            let kp = Keypair::generate();
            node.routing_table.add_node(NodeInfo {
                identity: kp.identity(),
                addr: NodeAddr {
                    protocol: "quic".into(),
                    address: format!("10.0.0.{i}:4433"),
                },
                last_seen: now_micros(),
            });
        }

        let sender = Keypair::generate();
        let find = FindNode {
            sender: sender.identity(),
            sender_addr: NodeAddr {
                protocol: "quic".into(),
                address: "10.0.0.99:4433".into(),
            },
            target: Hash::blake3(b"some target"),
        };

        let result = node.handle_find_node(&find);
        // 5 pre-added + sender (via update_routing_table) + self = 7
        assert_eq!(result.nodes.len(), 7);
    }

    #[test]
    fn handle_find_value_with_descriptors() {
        let mut node = make_node(4433);
        let kp = Keypair::generate();
        let rk = routing_key("compute");
        let now = now_micros();

        let desc = Descriptor::create(
            &kp,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![rk.clone()],
        )
        .unwrap();

        node.store.store_descriptor(desc).unwrap();

        let sender = Keypair::generate();
        let find = FindValue {
            sender: sender.identity(),
            sender_addr: NodeAddr {
                protocol: "quic".into(),
                address: "10.0.0.99:4433".into(),
            },
            key: rk,
            max_results: 20,
            filters: None,
        };

        let result = node.handle_find_value(&find);
        assert!(result.descriptors.is_some());
        assert!(result.nodes.is_none());
        assert_eq!(result.descriptors.unwrap().len(), 1);
    }

    #[test]
    fn handle_find_value_without_descriptors() {
        let mut node = make_node(4433);

        // Add nodes but no descriptors
        for i in 0..3 {
            let kp = Keypair::generate();
            node.routing_table.add_node(NodeInfo {
                identity: kp.identity(),
                addr: NodeAddr {
                    protocol: "quic".into(),
                    address: format!("10.0.0.{i}:4433"),
                },
                last_seen: now_micros(),
            });
        }

        let sender = Keypair::generate();
        let find = FindValue {
            sender: sender.identity(),
            sender_addr: NodeAddr {
                protocol: "quic".into(),
                address: "10.0.0.99:4433".into(),
            },
            key: routing_key("nonexistent"),
            max_results: 20,
            filters: None,
        };

        let result = node.handle_find_value(&find);
        assert!(result.descriptors.is_none());
        assert!(result.nodes.is_some());
        // 3 pre-added + sender (via update_routing_table) = 4
        assert_eq!(result.nodes.unwrap().len(), 4);
    }

    #[test]
    fn handle_find_value_with_filters() {
        let mut node = make_node(4433);
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let rk = routing_key("compute");
        let now = now_micros();

        for kp in [&kp1, &kp2] {
            let desc = Descriptor::create(
                kp,
                schema_hash("core/capability"),
                "topic".into(),
                b"payload".to_vec(),
                now,
                1,
                3600,
                vec![rk.clone()],
            )
            .unwrap();
            node.store.store_descriptor(desc).unwrap();
        }

        let sender = Keypair::generate();
        let find = FindValue {
            sender: sender.identity(),
            sender_addr: NodeAddr {
                protocol: "quic".into(),
                address: "10.0.0.99:4433".into(),
            },
            key: rk,
            max_results: 20,
            filters: Some(FilterSet {
                publisher: Some(kp1.identity()),
                ..Default::default()
            }),
        };

        let result = node.handle_find_value(&find);
        let descs = result.descriptors.unwrap();
        assert_eq!(descs.len(), 1);
        assert_eq!(descs[0].publisher, kp1.identity());
    }

    #[tokio::test]
    async fn bootstrap_populates_routing_table() {
        let transport = MockTransport::new();

        // Create a seed node with some known peers
        let mut seed = make_node(5000);
        for i in 0..5 {
            let kp = Keypair::generate();
            seed.routing_table.add_node(NodeInfo {
                identity: kp.identity(),
                addr: NodeAddr {
                    protocol: "quic".into(),
                    address: format!("10.0.0.{i}:4433"),
                },
                last_seen: now_micros(),
            });
        }
        let seed_addr = seed.addr().clone();
        transport.register(seed);

        // Bootstrap our node
        let mut node = make_node(4434);
        let discovered = node.bootstrap(&[seed_addr], &transport).await.unwrap();
        assert!(discovered > 0);
        assert!(node.routing_table.len() > 0);
    }

    #[tokio::test]
    async fn lookup_value_finds_descriptors() {
        let transport = MockTransport::new();
        let rk = routing_key("compute/inference");

        // Create a node that has the descriptor
        let mut holder = make_node(5001);
        let publisher_kp = Keypair::generate();
        let now = now_micros();
        let desc = Descriptor::create(
            &publisher_kp,
            schema_hash("core/capability"),
            "text-gen".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        holder.store.store_descriptor(desc.clone()).unwrap();
        let holder_identity = holder.identity().clone();
        let holder_addr = holder.addr().clone();
        transport.register(holder);

        // Our node knows about the holder
        let mut node = make_node(4435);
        node.routing_table.add_node(NodeInfo {
            identity: holder_identity,
            addr: holder_addr,
            last_seen: now,
        });

        let results = node.lookup_value(&rk, &transport).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, desc.id);
    }

    #[tokio::test]
    async fn lookup_value_follows_closer_nodes() {
        let transport = MockTransport::new();
        let rk = routing_key("compute/inference");

        // Node B has the descriptor
        let mut node_b = make_node(5002);
        let publisher_kp = Keypair::generate();
        let now = now_micros();
        let desc = Descriptor::create(
            &publisher_kp,
            schema_hash("core/capability"),
            "topic".into(),
            b"payload".to_vec(),
            now,
            1,
            3600,
            vec![rk.clone()],
        )
        .unwrap();
        node_b.store.store_descriptor(desc.clone()).unwrap();
        let node_b_identity = node_b.identity().clone();
        let node_b_addr = node_b.addr().clone();

        // Node A knows about node B but doesn't have the descriptor
        let mut node_a = make_node(5003);
        node_a.routing_table.add_node(NodeInfo {
            identity: node_b_identity.clone(),
            addr: node_b_addr.clone(),
            last_seen: now,
        });
        let node_a_identity = node_a.identity().clone();
        let node_a_addr = node_a.addr().clone();

        transport.register(node_a);
        transport.register(node_b);

        // Our node only knows about node A
        let mut our_node = make_node(4436);
        our_node.routing_table.add_node(NodeInfo {
            identity: node_a_identity,
            addr: node_a_addr,
            last_seen: now,
        });

        let results = our_node.lookup_value(&rk, &transport).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, desc.id);
    }

    #[tokio::test]
    async fn lookup_value_empty_routing_table() {
        let transport = MockTransport::new();
        let rk = routing_key("nonexistent");

        let mut node = make_node(4437);
        let results = node.lookup_value(&rk, &transport).await.unwrap();
        assert!(results.is_empty());
    }
}
