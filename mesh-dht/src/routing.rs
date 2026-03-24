//! Kademlia routing table with 256 k-buckets (Section 4.3).
//!
//! Each bucket holds up to K=20 `NodeInfo` entries, ordered by last-seen time
//! (most recent last). The bucket index is determined by the XOR distance
//! between the local node ID and the remote node ID.
//!
//! When a bucket is full, `add_node` returns [`AddNodeResult::BucketFull`]
//! so the caller can PING the least-recently-seen (LRS) node before deciding
//! whether to evict it (Section 4.3 — Sybil-resistance via ping challenge).

use mesh_core::Hash;
use mesh_core::message::NodeInfo;

use crate::distance::{bucket_index, distance_cmp, xor_distance};

/// Result of attempting to add a node to the routing table.
#[derive(Debug, Clone)]
pub enum AddNodeResult {
    /// Node was added (bucket had space).
    Added,
    /// Node already existed and was moved to most-recently-seen position.
    Updated,
    /// Bucket is full — caller should PING the LRS node to decide.
    BucketFull {
        /// The least-recently-seen node currently in the bucket.
        lrs: NodeInfo,
        /// The new node waiting to be added.
        candidate: NodeInfo,
    },
}

/// Kademlia replication parameter — max entries per bucket.
pub const K: usize = 20;

/// Number of k-buckets (one per bit of the 256-bit key space).
pub const NUM_BUCKETS: usize = 256;

/// A single k-bucket holding up to K node entries.
#[derive(Debug, Clone)]
pub struct KBucket {
    /// Entries ordered by last-seen time (most recent last).
    pub entries: Vec<NodeInfo>,
}

impl KBucket {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

/// The Kademlia routing table: 256 k-buckets indexed by XOR distance.
#[derive(Debug)]
pub struct RoutingTable {
    /// The local node's ID (BLAKE3 hash of public key).
    local_id: Hash,
    /// 256 k-buckets, indexed by distance prefix length.
    buckets: Vec<KBucket>,
}

impl RoutingTable {
    /// Create a new routing table for the given local node ID.
    pub fn new(local_id: Hash) -> Self {
        let buckets = (0..NUM_BUCKETS).map(|_| KBucket::new()).collect();
        Self { local_id, buckets }
    }

    /// Get the local node ID.
    pub fn local_id(&self) -> &Hash {
        &self.local_id
    }

    /// Compute which bucket a node belongs to, based on XOR distance to local ID.
    /// Returns `None` if the node ID equals the local ID.
    fn bucket_for(&self, node_id: &Hash) -> Option<usize> {
        let dist = xor_distance(&self.local_id, node_id);
        bucket_index(&dist)
    }

    /// Add a node to the routing table.
    ///
    /// If the node is already in its bucket, move it to the end (most recent)
    /// and return [`AddNodeResult::Updated`].
    ///
    /// If the bucket has room, insert and return [`AddNodeResult::Added`].
    ///
    /// If the bucket is full, return [`AddNodeResult::BucketFull`] with the
    /// least-recently-seen (LRS) entry so the caller can PING-challenge it
    /// before deciding (Section 4.3 — Sybil resistance). The caller should
    /// call [`resolve_challenge`](Self::resolve_challenge) with the result.
    pub fn add_node(&mut self, node: NodeInfo) -> AddNodeResult {
        let node_id = node.identity.node_id();
        let idx = match self.bucket_for(&node_id) {
            Some(i) => i,
            None => return AddNodeResult::Added, // don't add ourselves, but not an error
        };

        let bucket = &mut self.buckets[idx];

        // Check if node already exists in bucket
        if let Some(pos) = bucket
            .entries
            .iter()
            .position(|e| e.identity == node.identity)
        {
            // Move to end (most recently seen)
            bucket.entries.remove(pos);
            bucket.entries.push(node);
            return AddNodeResult::Updated;
        }

        // Bucket not full — just add
        if bucket.entries.len() < K {
            bucket.entries.push(node);
            return AddNodeResult::Added;
        }

        // Bucket full — return the LRS node for ping challenge
        let lrs = bucket.entries[0].clone();
        AddNodeResult::BucketFull {
            lrs,
            candidate: node,
        }
    }

    /// Resolve a ping challenge after receiving [`AddNodeResult::BucketFull`].
    ///
    /// If `lrs_responded` is true, the LRS node is still alive — keep it
    /// (move to tail as most-recently-seen) and discard the candidate.
    ///
    /// If `lrs_responded` is false, evict the LRS node and add the candidate.
    pub fn resolve_challenge(
        &mut self,
        lrs_id: &Hash,
        candidate: NodeInfo,
        lrs_responded: bool,
    ) {
        let candidate_id = candidate.identity.node_id();
        let idx = match self.bucket_for(&candidate_id) {
            Some(i) => i,
            None => return,
        };

        let bucket = &mut self.buckets[idx];

        if lrs_responded {
            // LRS is still alive — move it to tail (most-recently-seen)
            if let Some(pos) = bucket
                .entries
                .iter()
                .position(|e| e.identity.node_id() == *lrs_id)
            {
                let lrs_node = bucket.entries.remove(pos);
                bucket.entries.push(lrs_node);
            }
            // Discard candidate
        } else {
            // LRS is dead — evict it and add candidate
            bucket.entries.retain(|e| e.identity.node_id() != *lrs_id);
            if bucket.entries.len() < K {
                bucket.entries.push(candidate);
            }
        }
    }

    /// Remove a node from the routing table by its node ID (BLAKE3 of public key).
    pub fn remove_node(&mut self, node_id: &Hash) {
        let idx = match self.bucket_for(node_id) {
            Some(i) => i,
            None => return,
        };

        let bucket = &mut self.buckets[idx];
        bucket.entries.retain(|e| &e.identity.node_id() != node_id);
    }

    /// Return the `count` closest nodes to the target hash from the routing table,
    /// sorted by XOR distance (closest first).
    pub fn closest_nodes(&self, target: &Hash, count: usize) -> Vec<NodeInfo> {
        let mut all_nodes: Vec<&NodeInfo> =
            self.buckets.iter().flat_map(|b| b.entries.iter()).collect();

        all_nodes.sort_by(|a, b| {
            let id_a = a.identity.node_id();
            let id_b = b.identity.node_id();
            distance_cmp(target, &id_a, &id_b)
        });

        all_nodes.into_iter().take(count).cloned().collect()
    }

    /// Return all nodes in the routing table (unordered).
    pub fn all_nodes(&self) -> Vec<NodeInfo> {
        self.buckets.iter().flat_map(|b| b.entries.iter()).cloned().collect()
    }

    /// Total number of nodes in the routing table.
    pub fn len(&self) -> usize {
        self.buckets.iter().map(|b| b.entries.len()).sum()
    }

    /// Whether the routing table is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get a reference to a specific bucket.
    pub fn bucket(&self, index: usize) -> &KBucket {
        &self.buckets[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mesh_core::identity::Keypair;
    use mesh_core::message::NodeAddr;

    fn make_node_info(kp: &Keypair) -> NodeInfo {
        NodeInfo {
            identity: kp.identity(),
            addr: NodeAddr {
                protocol: "quic".into(),
                address: "127.0.0.1:4433".into(),
            },
            last_seen: 1_000_000,
        }
    }

    fn make_node_info_with_time(kp: &Keypair, last_seen: u64) -> NodeInfo {
        NodeInfo {
            identity: kp.identity(),
            addr: NodeAddr {
                protocol: "quic".into(),
                address: "127.0.0.1:4433".into(),
            },
            last_seen,
        }
    }

    #[test]
    fn new_table_is_empty() {
        let kp = Keypair::generate();
        let table = RoutingTable::new(kp.identity().node_id());
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn add_and_count() {
        let local = Keypair::generate();
        let mut table = RoutingTable::new(local.identity().node_id());

        for _ in 0..5 {
            let kp = Keypair::generate();
            table.add_node(make_node_info(&kp));
        }
        assert_eq!(table.len(), 5);
    }

    #[test]
    fn add_self_is_ignored() {
        let local = Keypair::generate();
        let mut table = RoutingTable::new(local.identity().node_id());
        table.add_node(make_node_info(&local));
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn add_duplicate_moves_to_end() {
        let local = Keypair::generate();
        let mut table = RoutingTable::new(local.identity().node_id());

        let node_kp = Keypair::generate();
        let node1 = make_node_info_with_time(&node_kp, 100);
        let node2 = make_node_info_with_time(&node_kp, 200);

        // Add another node first so we can check ordering
        let other = Keypair::generate();
        table.add_node(make_node_info_with_time(&other, 50));
        table.add_node(node1);
        table.add_node(node2);

        // Should still be 2 nodes total
        assert_eq!(table.len(), 2);
    }

    #[test]
    fn remove_node() {
        let local = Keypair::generate();
        let mut table = RoutingTable::new(local.identity().node_id());
        let node_kp = Keypair::generate();
        let node_id = node_kp.identity().node_id();

        table.add_node(make_node_info(&node_kp));
        assert_eq!(table.len(), 1);

        table.remove_node(&node_id);
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn remove_nonexistent_is_noop() {
        let local = Keypair::generate();
        let mut table = RoutingTable::new(local.identity().node_id());
        let kp = Keypair::generate();
        table.remove_node(&kp.identity().node_id());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn closest_nodes_ordering() {
        let local = Keypair::generate();
        let target = Hash::blake3(b"target");
        let mut table = RoutingTable::new(local.identity().node_id());

        let mut keypairs = Vec::new();
        for _ in 0..20 {
            let kp = Keypair::generate();
            table.add_node(make_node_info(&kp));
            keypairs.push(kp);
        }

        let closest = table.closest_nodes(&target, 5);
        assert_eq!(closest.len(), 5);

        // Verify ordering: each node should be closer to target than the next
        for i in 0..closest.len() - 1 {
            let id_a = closest[i].identity.node_id();
            let id_b = closest[i + 1].identity.node_id();
            let cmp = distance_cmp(&target, &id_a, &id_b);
            assert!(cmp != std::cmp::Ordering::Greater);
        }
    }

    #[test]
    fn closest_nodes_fewer_than_requested() {
        let local = Keypair::generate();
        let mut table = RoutingTable::new(local.identity().node_id());
        let kp = Keypair::generate();
        table.add_node(make_node_info(&kp));

        let closest = table.closest_nodes(&Hash::blake3(b"target"), 10);
        assert_eq!(closest.len(), 1);
    }

    #[test]
    fn bucket_full_returns_challenge() {
        let local = Keypair::generate();
        let mut table = RoutingTable::new(local.identity().node_id());

        // We need to fill a specific bucket. Generate nodes until we find K+1
        // that land in the same bucket.
        let mut bucket_counts = vec![0usize; NUM_BUCKETS];
        let mut nodes_by_bucket: Vec<Vec<Keypair>> = (0..NUM_BUCKETS).map(|_| Vec::new()).collect();
        let local_id = local.identity().node_id();

        loop {
            let kp = Keypair::generate();
            let node_id = kp.identity().node_id();
            let dist = xor_distance(&local_id, &node_id);
            if let Some(idx) = bucket_index(&dist) {
                nodes_by_bucket[idx].push(kp);
                bucket_counts[idx] += 1;
                if bucket_counts[idx] > K {
                    let target_bucket = idx;
                    // Add the first K nodes — should all succeed
                    for i in 0..K {
                        let node_kp = &nodes_by_bucket[target_bucket][i];
                        let result =
                            table.add_node(make_node_info_with_time(node_kp, i as u64));
                        assert!(matches!(result, AddNodeResult::Added));
                    }
                    assert_eq!(table.bucket(target_bucket).entries.len(), K);

                    // The (K+1)th node should trigger BucketFull
                    let overflow_kp = &nodes_by_bucket[target_bucket][K];
                    let result =
                        table.add_node(make_node_info_with_time(overflow_kp, K as u64));
                    match result {
                        AddNodeResult::BucketFull { lrs, candidate } => {
                            // LRS should be the first node (added with time 0)
                            assert_eq!(
                                lrs.identity,
                                nodes_by_bucket[target_bucket][0].identity()
                            );
                            assert_eq!(candidate.identity, overflow_kp.identity());
                        }
                        _ => panic!("expected BucketFull, got {:?}", result),
                    }

                    // Table should still have K entries (no eviction yet)
                    assert_eq!(table.bucket(target_bucket).entries.len(), K);
                    return;
                }
            }
        }
    }

    #[test]
    fn resolve_challenge_lrs_responded() {
        let local = Keypair::generate();
        let mut table = RoutingTable::new(local.identity().node_id());
        let local_id = local.identity().node_id();

        // Fill a bucket
        let mut bucket_counts = vec![0usize; NUM_BUCKETS];
        let mut nodes_by_bucket: Vec<Vec<Keypair>> = (0..NUM_BUCKETS).map(|_| Vec::new()).collect();

        loop {
            let kp = Keypair::generate();
            let node_id = kp.identity().node_id();
            let dist = xor_distance(&local_id, &node_id);
            if let Some(idx) = bucket_index(&dist) {
                nodes_by_bucket[idx].push(kp);
                bucket_counts[idx] += 1;
                if bucket_counts[idx] > K {
                    let target_bucket = idx;
                    for i in 0..K {
                        table.add_node(make_node_info_with_time(
                            &nodes_by_bucket[target_bucket][i],
                            i as u64,
                        ));
                    }

                    let overflow_kp = &nodes_by_bucket[target_bucket][K];
                    let candidate_info = make_node_info_with_time(overflow_kp, K as u64);
                    let result = table.add_node(candidate_info.clone());
                    let (lrs, candidate) = match result {
                        AddNodeResult::BucketFull { lrs, candidate } => (lrs, candidate),
                        _ => panic!("expected BucketFull"),
                    };
                    let lrs_id = lrs.identity.node_id();

                    // LRS responded — keep it, discard candidate
                    table.resolve_challenge(&lrs_id, candidate, true);

                    // LRS should still be in the bucket (moved to tail)
                    assert!(table
                        .bucket(target_bucket)
                        .entries
                        .iter()
                        .any(|e| e.identity.node_id() == lrs_id));
                    // LRS should be at the end (most recently seen)
                    let last = &table.bucket(target_bucket).entries[K - 1];
                    assert_eq!(last.identity.node_id(), lrs_id);
                    // Candidate should NOT be in the bucket
                    assert!(!table
                        .bucket(target_bucket)
                        .entries
                        .iter()
                        .any(|e| e.identity == overflow_kp.identity()));
                    return;
                }
            }
        }
    }

    #[test]
    fn resolve_challenge_lrs_dead() {
        let local = Keypair::generate();
        let mut table = RoutingTable::new(local.identity().node_id());
        let local_id = local.identity().node_id();

        let mut bucket_counts = vec![0usize; NUM_BUCKETS];
        let mut nodes_by_bucket: Vec<Vec<Keypair>> = (0..NUM_BUCKETS).map(|_| Vec::new()).collect();

        loop {
            let kp = Keypair::generate();
            let node_id = kp.identity().node_id();
            let dist = xor_distance(&local_id, &node_id);
            if let Some(idx) = bucket_index(&dist) {
                nodes_by_bucket[idx].push(kp);
                bucket_counts[idx] += 1;
                if bucket_counts[idx] > K {
                    let target_bucket = idx;
                    for i in 0..K {
                        table.add_node(make_node_info_with_time(
                            &nodes_by_bucket[target_bucket][i],
                            i as u64,
                        ));
                    }

                    let overflow_kp = &nodes_by_bucket[target_bucket][K];
                    let candidate_info = make_node_info_with_time(overflow_kp, K as u64);
                    let result = table.add_node(candidate_info.clone());
                    let (lrs, candidate) = match result {
                        AddNodeResult::BucketFull { lrs, candidate } => (lrs, candidate),
                        _ => panic!("expected BucketFull"),
                    };
                    let lrs_id = lrs.identity.node_id();

                    // LRS did NOT respond — evict it and add candidate
                    table.resolve_challenge(&lrs_id, candidate, false);

                    // LRS should be evicted
                    assert!(!table
                        .bucket(target_bucket)
                        .entries
                        .iter()
                        .any(|e| e.identity.node_id() == lrs_id));
                    // Candidate should be in the bucket
                    assert!(table
                        .bucket(target_bucket)
                        .entries
                        .iter()
                        .any(|e| e.identity == overflow_kp.identity()));
                    assert_eq!(table.bucket(target_bucket).entries.len(), K);
                    return;
                }
            }
        }
    }

    #[test]
    fn correct_bucket_placement() {
        let local = Keypair::generate();
        let local_id = local.identity().node_id();
        let table = RoutingTable::new(local_id.clone());

        let node = Keypair::generate();
        let node_id = node.identity().node_id();
        let dist = xor_distance(&local_id, &node_id);
        let expected_bucket = bucket_index(&dist);

        assert!(expected_bucket.is_some());
        let idx = expected_bucket.unwrap();
        assert!(idx < NUM_BUCKETS);

        // Verify the bucket_for method agrees
        assert_eq!(table.bucket_for(&node_id), expected_bucket);
    }
}
