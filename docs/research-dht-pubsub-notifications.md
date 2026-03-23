# Research: Pub/Sub Notification Models Over DHT Networks

**Date:** 2026-03-22
**Context:** Capability mesh protocol with Kademlia DHT, TTL-based capability descriptors, poll-based FIND_VALUE discovery. Evaluating options for adding real-time notifications as a future overlay layer.

---

## 1. System-by-System Findings

### 1.1 IPFS / libp2p GossipSub

**Architecture:** GossipSub is a *separate overlay* that runs alongside the Kademlia DHT — not on top of it. The DHT handles peer discovery and content routing; GossipSub manages its own topic-based mesh for message propagation.

**How it works:**
- Each peer maintains a **topic mesh** of D peers per topic (typically D_lo=6, D_hi=12).
- Subscribing to a topic: peer announces SUBSCRIBE to connected peers, then GRAFTs D peers into a local mesh for that topic.
- **Eager push:** Messages are forwarded in full to all mesh peers (low latency).
- **Lazy push (gossip):** On each heartbeat (~1s), IHAVE messages (containing only message IDs) are sent to random non-mesh peers. If a peer sees an IHAVE for a message it missed, it responds with IWANT to pull the full message.
- **Fanout map:** Peers can publish to topics they don't subscribe to by maintaining a set of D peers in that topic.

**Churn handling:**
- Heartbeat detects disconnected peers, removes them from mesh, GRAFTs replacements.
- v1.1 adds peer scoring: unreliable peers get negative scores and are pruned.
- Flood publishing (v1.1): publishers send to ALL known topic peers, not just mesh, for reliability.
- PRUNE messages include peer exchange (PX): pruned peers receive a list of alternative peers to connect to.

**Key takeaway:** PubSub and DHT are complementary layers. The DHT feeds peers into the GossipSub router; the router manages its own topology. This is the most mature model for "notifications alongside a DHT."

### 1.2 Ethereum devp2p

**Architecture:** The Kademlia-based discovery protocol (discv5) is used *only* for finding peers. All notifications flow over direct, persistent RLPx (encrypted TCP) connections to a stable peer set (25-50 peers).

**Notification model — announce-then-fetch:**
- **Transactions:** `NewPooledTransactionHashes` pushes tx hashes to peers. Peers pull full tx data via `GetPooledTransactions`.
- **Blocks:** `NewBlock` pushes the full block to sqrt(N) peers. `NewBlockHashes` announces hash+number to the rest. Recipients pull headers/bodies if needed.

**Churn handling:** Relies on maintaining a stable peer set. Discovery protocol continuously finds new peers to replace disconnected ones. No subscription state survives disconnection.

**Key takeaway:** The announce-then-fetch pattern (push a small identifier, let the receiver pull data) is bandwidth-efficient and maps well to capability hash announcements. The sqrt(N) broadcast pattern is a useful optimization for high-value updates.

### 1.3 BitTorrent BEP 46 (Mutable Items)

**Architecture:** Pure DHT storage with Ed25519-signed mutable items, sequence numbers for ordering, and TTL-based expiry. This is the closest analog to the current mesh protocol design.

**Notification model: NONE — pure polling.**
- Consumers periodically issue DHT GET requests for a target ID.
- They check if the sequence number has increased.
- Both publishers and consumers must periodically PUT to keep items alive.

**Key takeaway:** BEP 46 demonstrates the fundamental limitation of Kademlia for pub/sub: the DHT has no built-in notification mechanism. Polling is the only option within the DHT itself. This confirms that notifications *must* be layered on top.

### 1.4 Scuttlebutt / SSB

**Architecture:** No DHT at all. Uses social-graph-based gossip replication over direct peer connections (pubs, LAN discovery, rooms).

**Notification model — two modes:**

1. **createHistoryStream (classic):** Pull-based RPC. Peer requests messages from sequence N onward. With `live=true`, the stream stays open and new messages are pushed immediately — this is the closest to "subscribe and be notified."

2. **EBT Replication (Epidemic Broadcast Trees / Plumtree-inspired):**
   - Peers exchange **vector clocks** (feedId -> latestSequence).
   - Messages are pushed eagerly through a spanning tree (fast path).
   - Lazy push (vector clock metadata only) provides redundancy and repair.
   - **Request skipping:** On reconnection, peers compare stored vector clocks to avoid re-sending known state.

**Churn handling:** Deterministic catch-up via append-only logs and vector clocks. On reconnection, peers exchange clocks and sync only the delta. Hop-based replication bounds what each peer tracks (e.g., 2-3 hops in the social graph).

**Key takeaway:** The EBT/Plumtree model of eager-push over a spanning tree with lazy-push backup is highly efficient and self-healing. Vector clock exchange on reconnection solves the "what did I miss?" problem cleanly.

### 1.5 Other Systems

**PolderCast:** Topic-based pub/sub using three overlay layers — Cyclon (random peer sampling), Vicinity (cluster by topic interest), and Rings (per-topic dissemination rings). Self-organizes by topic affinity. Designed for very high topic cardinality.

**Plumtree (Epidemic Broadcast Trees):** Core algorithm used by SSB's EBT and Riak. Start with eager push (flood), prune redundant links to lazy. Result: a near-optimal spanning tree with lazy backup links for repair. Self-healing: if an eager link fails, a lazy link is promoted.

**Waku (Status.im):** Built on GossipSub relay with added store and filter protocols. Filter protocol lets light nodes subscribe to full nodes for specific content topics. Store protocol provides historical message retrieval for catch-up after being offline. Relevant pattern: separating relay, filter, and store into composable sub-protocols.

---

## 2. Patterns for Adding Pub/Sub on Top of Kademlia Without Modifying the DHT

Every production system studied uses the same fundamental approach: **the DHT is for discovery and storage; notifications are a separate overlay.**

### Pattern A: Topic-Based Gossip Mesh (GossipSub model)
- Peers interested in a capability topic (e.g., a capability type or namespace) join a mesh.
- Publishers push update notifications through the mesh.
- DHT remains the source of truth; mesh is for real-time notification.
- Requires: mesh management (GRAFT/PRUNE), heartbeat, peer scoring.

### Pattern B: Announce-Then-Fetch (Ethereum model)
- When a capability descriptor is published/updated, the publisher announces the capability hash to connected peers.
- Interested peers fetch the full descriptor from the DHT via FIND_VALUE.
- Lower complexity than full mesh, but requires maintaining a connected peer set.

### Pattern C: Watch Keys via DHT Proximity (novel, lighter weight)
- Peers that are "close" to a key in the Kademlia keyspace (i.e., the nodes that store it) can act as notification relays.
- When a STORE arrives for a key, the storing node pushes a notification to any peer that has registered a WATCH for that key.
- Requires a small DHT extension (WATCH/NOTIFY messages) but keeps notifications close to existing DHT topology.
- Trade-off: ties notification routing to DHT topology, which may not match interest topology.

### Pattern D: Live Streams (Scuttlebutt model)
- When a peer discovers a capability via FIND_VALUE, it opens a persistent stream to the publisher or a nearby relay.
- New versions are pushed over the stream.
- Simple but creates O(subscribers) connections on the publisher.

---

## 3. Trade-off Analysis

| Approach | Latency | Bandwidth | Churn Resilience | Complexity | Scales With |
|---|---|---|---|---|---|
| **Polling (current)** | High (poll interval) | Wasteful at scale | Excellent (stateless) | Minimal | Topics polled |
| **Gossip mesh (GossipSub)** | Low (~1s) | Moderate (mesh + gossip) | Good (heartbeat repair) | High | Subscribers per topic |
| **Announce-then-fetch** | Low-medium | Low (push hash only) | Moderate (peer set mgmt) | Medium | Connected peers |
| **DHT WATCH/NOTIFY** | Low | Low | Poor (watch state lost on churn) | Medium | Keys watched |
| **Live streams** | Lowest | Low per-stream | Poor (reconnect needed) | Low | Subscribers per publisher |

### Gossip-Based Notification
- **Pro:** Decoupled from publisher, survives publisher churn, self-healing mesh.
- **Con:** Overhead per topic, requires mesh management, non-trivial protocol surface.

### Polling
- **Pro:** Zero additional protocol complexity, stateless, works with any DHT.
- **Con:** Latency proportional to poll interval, wasted bandwidth on unchanged data.

### Dedicated Subscription Channels (Live Streams / WATCH)
- **Pro:** Lowest latency, minimal bandwidth.
- **Con:** State must be maintained per subscription, subscriber churn causes reconnection storms, publisher becomes a bottleneck.

---

## 4. Recommendation for the Capability Mesh Protocol

### Phase 1: Optimized Polling (immediate, no protocol change)
- Add **conditional FIND_VALUE**: include a version/sequence number or hash in the request. Responding node returns NOT_CHANGED if the value hasn't been updated. This eliminates transferring unchanged descriptors.
- Add **exponential backoff with jitter** on poll intervals: poll frequently after first discovery, decay over time.
- This is what BEP 46 does and it works adequately for moderate update rates.

### Phase 2: Announce-Then-Fetch Overlay (medium-term, additive layer)
- When a capability descriptor is STORE'd to the DHT, the publishing node also sends a lightweight `CAPABILITY_UPDATED(key, version, hash)` announcement to its connected peers.
- Peers maintain an **interest filter** (e.g., Bloom filter of capability types/namespaces they care about). They forward matching announcements and drop non-matching ones.
- Receiving peers fetch the full descriptor from the DHT only if interested and if the version is newer than what they have cached.
- This is the Ethereum announce-then-fetch pattern adapted for capabilities. It does not modify the DHT protocol — it's a separate message type on the same transport.

### Phase 3: GossipSub-Style Topic Mesh (long-term, for high-frequency topics)
- For capability namespaces with many subscribers and frequent updates, spin up a GossipSub-style topic mesh.
- Use the DHT to bootstrap mesh membership (FIND_VALUE for a "topic registry" key returns a list of peers in that mesh).
- Mesh handles real-time notifications; DHT remains the durable store and source of truth.
- Consider the Plumtree/EBT approach (eager push over spanning tree, lazy push for repair) as a simpler alternative to full GossipSub if the mesh is small.

### Churn Strategy (all phases)
- **DHT TTL republish** already handles data persistence through churn.
- **Notification state is ephemeral:** if a subscriber goes offline, it catches up via polling or conditional FIND_VALUE on reconnection. No notification state is persisted in the DHT.
- **Vector clock catch-up** (SSB model): on reconnection, peers exchange a compact summary of "what I last saw" and sync the delta. For capabilities, this could be a map of `{capability_key: last_known_version}`.

### Why This Layered Approach
- Phase 1 requires zero protocol changes and immediately reduces polling waste.
- Phase 2 adds sub-second notification latency for most cases with a single new message type, and degrades gracefully to polling if the overlay is unavailable.
- Phase 3 is only needed if specific capability topics become high-traffic, and can be adopted selectively per topic.
- At every phase, the DHT remains the authoritative store, and notifications are best-effort optimization. A node that misses a notification simply discovers the update on its next poll or catch-up sync.

---

## 5. Sources

- **GossipSub v1.0 spec:** github.com/libp2p/specs/pubsub/gossipsub/gossipsub-v1.0.md
- **GossipSub v1.1 spec:** github.com/libp2p/specs/pubsub/gossipsub/gossipsub-v1.1.md
- **Ethereum Wire Protocol (eth):** github.com/ethereum/devp2p/caps/eth.md
- **BitTorrent BEP 46:** bittorrent.org/beps/bep_0046.html
- **Scuttlebutt Protocol Guide:** ssbc.github.io/scuttlebutt-protocol-guide/
- **Plumtree (Epidemic Broadcast Trees):** Leitao et al., SRDS 2007
- **PolderCast:** Setty et al., Middleware 2012
- **Waku v2:** rfc.vac.dev/waku/standards/core/
