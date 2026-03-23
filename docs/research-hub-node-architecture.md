# Hub Node Architecture Research: P2P Infrastructure Patterns

**Date**: 2026-03-22
**Context**: Designing a hub node for Kademlia-based capability mesh DHT. Hub stores capability descriptors (max 64KB each), maintains full routing tables, peers with all other hubs (BGP-style), and will serve as commercial infrastructure (ISP model).

---

## 1. BitTorrent — High-Capacity DHT Nodes

### Bootstrap Nodes
- Hardcoded into clients as initial contact points for joining the DHT
- Run standard Kademlia protocol (BEP 5) — no special protocol privileges
- Differ only operationally: high uptime, stable IPs, large routing tables
- Routing tables use k-buckets (k=8), split on the node's own ID prefix — nodes know many nearby peers but few distant ones
- Node liveness: "good" if responded to query in last 15 min; "questionable" after 15 min inactivity; "bad" after multiple failed queries

### Super-Seeding (BEP 16)
- Bandwidth optimization, not a protocol role — masquerades as a normal peer
- Strategically reveals rarest pieces first, waits for propagation before sharing more
- Achieves ~105% upload efficiency vs 150-200% for normal seeding
- **Lesson**: Infrastructure nodes can optimize resource allocation without protocol changes by being strategically selective about what they serve

### Storage at Scale
- DHT stores only peer contact info (compact: 6 bytes per peer = IP+port)
- No eviction needed — entries are small and bounded by k-bucket size
- Routing table size is O(log n) buckets * k entries = manageable even at millions of nodes

### Relevance to Hub Design
- **Full routing table**: Hubs should maintain expanded k-buckets (k=20 or higher) for better lookup performance
- **Bootstrap role**: Hubs naturally serve as bootstrap nodes — hardcode hub list for new mesh joiners
- **Liveness tracking**: Adopt 15-min good/questionable/bad model for routing table maintenance

---

## 2. IPFS — Bootstrap Nodes & Pinning Services

### Bootstrap Nodes
- Default list hardcoded in IPFS implementations; users can configure custom bootstrap lists
- Bootstrap nodes are regular IPFS nodes with high uptime — no special protocol authority
- They serve as DHT entry points and relay connection-discovery starting points

### Relay Nodes
- Use libp2p Circuit Relay (covered in section 6) for NAT traversal
- Not storage-related — pure connectivity infrastructure

### Delegated Routing
- Nodes can offload DHT queries to a delegated routing server (HTTP API)
- Enables lightweight clients that don't participate in the DHT themselves
- **Key pattern**: Hub can offer delegated routing as a paid service tier

### Pinning Services (Pinata, Filebase, web3.storage/Storacha)
- Core model: "pin" a CID to prevent garbage collection — data persists on the service's nodes
- Pinning ≠ storage; it's a retention guarantee backed by infrastructure

### Pinata Multi-Tenancy Architecture
- **API Key isolation**: Per-account API keys with scoped permissions
- **Workspaces**: Team-based multi-tenancy — members invited by email, role-based access (Owner can remove members)
- **Per-plan rate limits**: Tiered API rate limits and storage quotas per pricing plan
- **Dedicated gateways**: Per-tenant IPFS gateways with 200+ edge cache locations; gateway access controls (IP allowlists, token auth)
- **File organization**: Groups, key-value metadata per file, file vectors (beta)
- **Billing hooks**: Per-account billing tied to storage used + API calls + gateway bandwidth

### Relevance to Hub Design
- **Pinning = "sponsored storage"**: Hub tenants pin capability descriptors — hub guarantees retention and replication
- **Dedicated gateways model**: Each tenant gets a scoped query endpoint with their own rate limits
- **Workspace model**: Good template for multi-tenant hub — account > workspace > API keys > file groups

---

## 3. Ethereum — Infrastructure Providers (Infura, Alchemy, QuickNode)

### Architecture Pattern
These providers turned "running a node" into an API product. Common architecture:

1. **Node Pool**: Multiple full/archive Ethereum nodes behind a load balancer
2. **API Gateway Layer**: Authenticates requests (API key in URL path), routes to appropriate node
3. **Caching Layer**: Hot cache for recent blocks, mempool data, frequently-queried contract state
4. **Rate Limiting**: Per-API-key token bucket, with burst allowances; tiered by plan
5. **Failover**: Infura's Decentralized Infrastructure Network (DIN) forwards to partner nodes if primary is unavailable

### Multi-Tenancy Model
- **Tenant = API Key**: Each project gets a unique API key; all billing, rate limiting, and analytics scoped to that key
- **Per-method pricing**: Some providers charge more for expensive methods (debug_traceCall, eth_getLogs with wide ranges)
- **Compute Units (CUs)**: Alchemy normalizes all methods to "compute units" — a `eth_blockNumber` costs 1 CU, a `eth_call` costs 26 CUs. Rate limits and billing expressed in CUs/second
- **Archive vs Full**: Different pricing tiers for archive data access (historical state at any block)

### Monitoring & Observability
- Per-tenant dashboards: request counts, error rates, latency percentiles, method breakdown
- Real-time alerts on rate limit hits, error spikes
- Usage analytics for billing reconciliation

### Transition Path: Node → Service
1. Start by running nodes for internal use
2. Expose API with key-based auth
3. Add rate limiting and usage tracking
4. Build billing integration
5. Add caching/optimization layer for cost efficiency
6. Add multi-region for latency
7. Add failover partnerships (DIN model)

### Relevance to Hub Design
- **Compute Unit model**: Normalize hub operations (STORE_CAPABILITY, FIND_CAPABILITY, ROUTE_QUERY) to weighted "mesh units" for fair billing
- **API key as tenant boundary**: Simple, proven pattern — embed tenant ID in every request
- **Caching layer is critical**: Hot capabilities (frequently queried) should be in-memory; cold in disk-backed store
- **DIN-style failover**: Hubs can forward to peer hubs if overloaded — natural fit for BGP-style peering

---

## 4. Tor — Relay Architecture & Directory Authorities

### Node Types (Strict Role Separation)
1. **Directory Authorities** (9 total, hardcoded IPs):
   - Vote hourly on network consensus — which relays are good, their bandwidth capacity, flags
   - Identity keys stored offline; medium-term signing keys used for daily operations
   - Publish signed consensus documents that all clients download on startup
   - **Quorum-based**: Majority must agree; compromise of minority cannot alter consensus

2. **Guard (Entry) Nodes** (~2,000):
   - First hop in a Tor circuit; sees client's real IP
   - Must earn "Guard" flag through sustained uptime and bandwidth
   - Clients pin to a small set of guards for months (reduces intersection attack surface)

3. **Middle Relays** (~4,000+):
   - Second hop; knows only guard and exit, not client or destination
   - Lowest barrier to entry — any stable relay qualifies

4. **Exit Nodes** (~1,000):
   - Final hop; makes the actual connection to the destination
   - Highest liability — operators face abuse complaints
   - Runs exit policies defining which ports/IPs they'll connect to

### Consensus Architecture
- Version 3 protocol: Authorities collectively vote → single signed consensus document
- Clients fetch consensus on startup, refresh periodically
- Directory caches (mirrors) distribute consensus documents to reduce authority load
- Consensus includes per-relay: flags (Guard, Exit, Stable, Fast, HSDir), bandwidth estimates, keys

### Storage Model
- Relay descriptors: ~2-4KB each, ~7,000 relays → ~20MB total network state
- Consensus document: compact — just flags and references, ~1-2MB
- Clients cache consensus + descriptors locally; incremental updates via diffs

### Relevance to Hub Design
- **Directory authority model**: Hub consensus on capability routing could use similar voting — hubs vote on network state, publish signed consensus
- **Flag system**: Assign hub-level flags (Stable, HighCapacity, Verified) based on observed behavior
- **Guard pinning pattern**: Clients should pin to a preferred hub for stability, with fallback list
- **Consensus caching**: Hub peers cache and redistribute consensus to reduce load on authorities
- **Signing key rotation**: Use offline root keys, online medium-term signing keys (exactly like Tor DAs)

---

## 5. Matrix — Federation at Scale

### Federation Architecture
- Homeservers exchange data via HTTPS: PDUs (persistent events), EDUs (ephemeral), and queries
- Transactions are batched: max 50 PDUs + 100 EDUs per transaction PUT
- Sending server retries until 200 OK before advancing to next transaction
- Events are signed by originating server — can be relayed through third parties

### Synapse Pro (Element's Commercial Offering)
- **Horizontal scaling via Kubernetes**: Worker processes handle different event types
- **Shared microservices**: Eliminate redundant data across workers → 80% resource savings
- **Shared data caches**: Significantly reduced RAM footprint (5x smaller than community Synapse)
- **Built in Rust**: Multi-core utilization for worker processes
- **Elastic scaling**: Auto-adjusts to demand spikes (e.g., Monday morning login surges) without restarts
- **High availability**: Multi-datacenter deployments with failover

### Multi-Tenant Hosting
- Element Server Suite Pro enables "high-density multi-tenancy"
- Multiple logical homeservers share physical infrastructure
- Per-tenant isolation at the application layer (each tenant = a homeserver with its own namespace)
- Cost savings scale with tenant count (shared caches, shared workers)

### Storage at Scale
- PostgreSQL is the backing store for Synapse
- Large homeservers (matrix.org) handle millions of rooms, billions of events
- Key optimization: state resolution algorithm determines current room state without replaying all events
- Media stored separately (often S3-compatible object storage)

### Relevance to Hub Design
- **Transaction batching**: Batch capability updates between hubs (like Matrix PDU batching) — max N updates per sync message
- **Shared worker model**: Hub can have specialized workers: routing-worker, storage-worker, query-worker, replication-worker
- **PostgreSQL + cache pattern**: Proven for millions of records — capabilities in PostgreSQL with hot-path cache in Redis/in-memory
- **Elastic scaling**: Kubernetes-native from the start; per-worker horizontal scaling
- **Namespace isolation for tenants**: Each tenant gets a namespace prefix; all queries scoped to namespace

---

## 6. libp2p — Circuit Relay v2

### Architecture
- Protocol split into two subprotocols:
  - **Hop protocol** (client→relay): Reserve resources, initiate relayed connections
  - **Stop protocol** (relay→target): Deliver incoming relayed connections

### Reservation System
- Peers explicitly reserve relay slots with `RESERVE` message
- Relay responds with reservation details:
  - `expire`: UTC Unix timestamp — client must refresh before expiry
  - `addrs`: Relay's public addresses for constructing circuit addresses
  - `voucher`: Signed reservation voucher — proof of reservation, distributable to peers

### Connection Limits (Abuse Prevention)
- **Per-connection limits** via `Limit` message:
  - `duration`: Max seconds for a relayed connection
  - `data`: Max bytes transferred through relay
- **Reservation limits**: Relay can refuse with `RESERVATION_REFUSED` if too many active reservations
- **ACL-based filtering**: `PERMISSION_DENIED` for peer filtering — relay operators control who can reserve
- **End-to-end encryption**: Relay cannot read/tamper with traffic (only routes encrypted bytes)

### Default Recommended Limits (from go-libp2p implementation)
- Max 128 reservations per relay
- Max 16 reservations per peer
- Reservation TTL: 1 hour
- Max relayed connection duration: 2 minutes
- Max relayed data: 128KB per connection
- Max circuit connections per relay: 300

### Relevance to Hub Design
- **Reservation model is directly applicable**: Tenants "reserve" hub capacity — storage slots, query bandwidth
- **Voucher pattern**: Tenants get signed vouchers they can distribute to their clients as proof of hub access
- **Explicit resource limits**: Every tenant reservation includes concrete limits (storage bytes, queries/sec, TTL)
- **ACL-based admission**: Hub operators maintain allowlists/denylists for tenant admission
- **Connection-level caps**: Per-query data limits prevent any single request from monopolizing hub resources

---

## Architectural Recommendations for Hub Node

### Storage Architecture

| Layer | Technology | Purpose | Capacity |
|-------|-----------|---------|----------|
| L1: Hot Cache | In-memory (e.g., Rust HashMap + LRU) | Frequently-queried capabilities, routing table | 1-10GB RAM |
| L2: Warm Store | Embedded KV (sled, RocksDB, LMDB) | Full capability index, recent descriptors | 100GB-1TB SSD |
| L3: Cold Archive | PostgreSQL or S3-compatible | Historical versions, audit trail, tenant metadata | Unbounded |

**Indexing**: B-tree on capability hash (Kademlia ID), secondary indexes on capability type, tenant ID, and semantic tags. Full-text index on capability descriptions for discovery queries.

**Eviction Strategy**: LRU at L1, TTL-based at L2 (capabilities must be refreshed by owner or tenant pin), no eviction at L3 (archival).

### Multi-Tenancy Model

```
Account
  └── Workspace (billing unit)
       ├── API Keys (scoped permissions: read/write/admin)
       ├── Capability Pins (retained descriptors)
       ├── Rate Limits (mesh-units/sec, storage bytes, query budget)
       └── Namespace Prefix (isolation boundary in DHT keyspace)
```

**Tenant Isolation**:
- Namespace prefix on all stored capabilities (`/tenant-id/capability-hash`)
- Per-tenant rate limiting using token bucket (like Infura/Alchemy API key model)
- Per-tenant storage quotas with soft/hard limits and alerting
- Shared infrastructure, logically isolated (Matrix Synapse Pro pattern)

**Billing Hooks**:
- Normalize all operations to "Mesh Units" (like Alchemy Compute Units):
  - `STORE_CAPABILITY`: 10 MU
  - `FIND_CAPABILITY`: 1 MU
  - `ROUTE_QUERY` (cross-hub): 5 MU
  - `PIN_CAPABILITY` (retention guarantee): 1 MU/day per 64KB
- Meter at API gateway layer; emit usage events to billing pipeline
- Webhook notifications at 80%/100% quota thresholds

### Rate Limiting & Abuse Prevention

1. **Per-tenant token bucket**: Configured rate (MU/sec) with burst allowance
2. **Per-IP connection limits**: Max connections per source IP (prevent Sybil floods)
3. **Per-capability write limits**: Max updates/hour per capability hash (prevent churn attacks)
4. **ACL admission control**: Tenant allowlist for hub reservation (libp2p pattern)
5. **Proof-of-work or stake for anonymous access**: Optional for non-tenant queries
6. **Circuit breaker**: Auto-shed load from misbehaving tenants/peers

### Monitoring & Observability

Standard infrastructure node metrics (following Infura/Alchemy patterns):

- **Per-tenant**: Request count, error rate, latency p50/p95/p99, MU consumption, storage used
- **Per-hub**: Routing table size, peer count, replication lag, query fanout ratio
- **Network-level**: Inter-hub sync latency, consensus health, capability count, churn rate
- **Alerting**: Rate limit breaches, storage quota warnings, peer disconnections, replication failures

Stack: Prometheus metrics → Grafana dashboards → PagerDuty/webhook alerts

### Hub Peering (BGP-Style)

Drawing from Tor's directory authority model and Matrix federation:

1. **Hub Discovery**: Hardcoded seed list (like Tor DAs / BT bootstrap nodes) + gossip protocol for new hubs
2. **Peering Sessions**: Persistent connections between hubs; exchange routing table diffs (not full state)
3. **Consensus**: Hubs vote on network state (active capabilities, hub flags, routing policy) — publish signed consensus document
4. **Hub Flags**: Earned through observed behavior — `Stable`, `HighCapacity`, `Verified`, `FullTable`
5. **Transaction Batching**: Max N capability updates per sync message (Matrix's 50 PDU limit pattern)
6. **Signing**: Offline root key → medium-term signing key for hub-to-hub messages (Tor DA pattern)

### Transition Path: Hub → Commercial Service

Following the Ethereum infrastructure provider playbook:

| Phase | Capability | Revenue Model |
|-------|-----------|---------------|
| 1. Community Hub | Open DHT participation, full routing | None (reputation building) |
| 2. Reliable Hub | SLA guarantees, monitoring, alerting | Freemium (free tier + paid) |
| 3. Managed Hub | Multi-tenant, API keys, dashboards | Per-tenant subscription |
| 4. Hub Platform | White-label hub hosting, custom domains | Platform fees |
| 5. Hub Network | Federated hub mesh with failover (DIN model) | Revenue sharing |

---

## Key Takeaways

1. **No special protocol privileges for hubs** — they're just better-resourced nodes running the same protocol (BitTorrent/IPFS pattern). Differentiation is operational, not protocol-level.

2. **Reservation + voucher model from libp2p is the right tenant primitive** — explicit resource reservation with signed vouchers, time-bounded, with concrete limits.

3. **Compute Unit normalization (Alchemy pattern) solves billing** — abstract all operations to a single unit; meter at the gateway; bill per unit.

4. **Directory authority consensus (Tor pattern) fits hub peering** — small set of hubs vote on network state; publish signed consensus; peers cache and redistribute.

5. **Shared-nothing workers with shared cache (Synapse Pro pattern) is the scaling model** — specialized workers behind a cache layer; Kubernetes-native; elastic scaling.

6. **Multi-tenancy is namespace isolation + API key scoping + rate limiting** — not separate infrastructure per tenant. Every system studied uses logical isolation on shared infrastructure.
