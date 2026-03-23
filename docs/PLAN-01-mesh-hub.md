# PLAN-01: Mesh Hub Design

**Status:** In Design
**Date:** 2026-03-22
**Depends on:** PROTOCOL.md (v0.1.0-draft)

---

## 1. Overview

A hub is a high-capacity mesh node that serves as backbone infrastructure for
the capability mesh. It speaks the same protocol as any node (8 message types,
QUIC transport, signed descriptors) but operates at a higher tier: full routing
tables, aggressive storage coverage, hub-to-hub peering, and multi-tenant
commercial service.

Hubs are the ISPs of the mesh. Their business model is providing reliable
capability discovery infrastructure to providers and consumers who need
guaranteed reachability.

### 1.1 Design Goals

1. **Protocol-native** — a hub is a mesh node, not a sidecar. Same binary
   lineage, same crate dependencies, extended behavior.
2. **Multi-tenant from day one** — accounts with multiple publisher identities,
   per-tenant quotas, isolation, billing hooks.
3. **Horizontally scalable** — a single hub binary handles a mid-size network;
   hub operators can scale out when needed.
4. **Observable** — metrics, health checks, and admin API are first-class.
5. **Reference implementation** — this design serves as the template for all
   future hub operators.

### 1.2 Crate Placement

```
mesh-protocol/
  mesh-core/          — types, serialization, hashing, signing
  mesh-dht/           — Kademlia implementation
  mesh-transport/     — QUIC transport (quinn-based)
  mesh-node/          — standard node binary
  mesh-hub/           — hub node binary (depends on mesh-node internals)
  mesh-client/        — lightweight client library
  mesh-schemas/       — core schema definitions and validators
```

`mesh-hub` depends on `mesh-core`, `mesh-dht`, `mesh-transport`, and
`mesh-schemas`. It does NOT depend on `mesh-node` as a library — they are
sibling binaries that share the lower crates.

### 1.3 Library Interface Requirements

mesh-hub MUST be usable as a library (`mesh-hub = { path = "../mesh-hub" }`)
in addition to shipping as a standalone binary. This enables downstream
projects to compose mesh-hub's components into custom binaries with
additional business logic (payment, premium features, custom policies)
without forking.

To support this, the following traits and extension points are required
in the upstream crates:

**mesh-dht: Storage trait abstraction**

The current `DescriptorStore` is a concrete in-memory `HashMap`. mesh-hub
needs disk-backed storage, and downstream projects need tenant-tagged
storage. Extract a trait:

```rust
/// Pluggable descriptor storage backend.
/// mesh-dht provides an in-memory default. mesh-hub provides disk-backed.
/// Downstream projects can wrap either with tenant tagging, caching, etc.
#[async_trait]
pub trait DescriptorStorage: Send + Sync {
    /// Store a descriptor. Returns true if stored (not a stale duplicate).
    async fn store(&self, descriptor: Descriptor) -> Result<bool>;

    /// Retrieve descriptors at a routing key. Excludes expired entries.
    async fn get(&self, routing_key: &Hash, now: u64) -> Vec<Descriptor>;

    /// Remove expired descriptors. Returns count removed.
    async fn evict_expired(&self, now: u64) -> usize;

    /// Check rate limit for a publisher. Returns true if allowed.
    async fn check_rate_limit(&self, publisher: &Identity) -> bool;
}
```

`DhtNode` becomes generic over storage: `DhtNode<S: DescriptorStorage>`.
The existing `DescriptorStore` becomes the default implementation.

**mesh-dht: Protocol hooks**

A hook trait that lets downstream code intercept protocol operations
without modifying core DHT logic:

```rust
/// Hooks for observing and intercepting protocol operations.
/// Default implementation is pass-through (no-op).
#[async_trait]
pub trait ProtocolHook: Send + Sync {
    /// Called before a STORE is accepted. Return Err to reject.
    async fn pre_store(
        &self,
        descriptor: &Descriptor,
    ) -> Result<()> {
        Ok(())
    }

    /// Called after a STORE is accepted.
    async fn post_store(
        &self,
        descriptor: &Descriptor,
    ) {}

    /// Called before a FIND_VALUE result is returned. Return Err to reject.
    async fn pre_query(
        &self,
        key: &Hash,
        requester: &Identity,
    ) -> Result<()> {
        Ok(())
    }

    /// Called after a FIND_VALUE result is returned.
    async fn post_query(
        &self,
        key: &Hash,
        requester: &Identity,
        result_count: usize,
    ) {}
}
```

`DhtNode` accepts an optional `Arc<dyn ProtocolHook>`. This is how
downstream projects implement MU metering, tenant-aware rate limiting,
and audit logging without touching core DHT code.

**mesh-hub: Runtime and manager APIs**

mesh-hub itself must expose its internals as a composable library:

```rust
/// The core hub runtime. Can be started, configured, and extended.
pub struct HubRuntime { ... }

impl HubRuntime {
    pub fn builder() -> HubBuilder;
    pub async fn start(&self) -> Result<()>;
    pub async fn shutdown(&self) -> Result<()>;

    pub fn storage(&self) -> &dyn DescriptorStorage;
    pub fn dht(&self) -> &DhtNode<impl DescriptorStorage>;
    pub fn tenant_manager(&self) -> &TenantManager;

    /// Returns the base admin API router. Downstream can merge
    /// additional routes (payment, portal, custom endpoints).
    pub fn admin_router(&self) -> axum::Router;
}
```

This means the mesh-hub binary is a thin wrapper:

```rust
// mesh-hub/src/main.rs
fn main() {
    let config = HubConfig::from_file("mesh-hub.toml");
    let hub = HubRuntime::builder()
        .config(config)
        .build();
    hub.start().await;
}
```

And a downstream project (e.g., PurposeBot hub) composes on top:

```rust
// downstream/src/main.rs
fn main() {
    let hub = HubRuntime::builder()
        .config(config)
        .hook(Arc::new(MeteringHook::new(...)))  // custom hook
        .build();

    let app = hub.admin_router()
        .merge(payment_routes(...))   // custom routes
        .merge(portal_routes(...));   // custom routes

    // start hub + custom HTTP server
}
```

**Summary of upstream changes required:**

| Change | Crate | Type | Priority |
|--------|-------|------|----------|
| Extract `DescriptorStorage` trait | mesh-dht | New trait + refactor | Critical |
| Add `ProtocolHook` trait | mesh-dht | New trait | Critical |
| Make `DhtNode` generic over storage | mesh-dht | Refactor | Critical |
| Expose `HubRuntime` with builder | mesh-hub | New API | Critical |
| Expose `TenantManager` API | mesh-hub | New API | High |
| Expose admin router as composable | mesh-hub | New API | High |
| Expose `DhtNode` and storage via runtime | mesh-hub | New API | High |

These changes benefit the open source project regardless of downstream
use — they make mesh-hub testable, configurable, and pluggable.

---

## 2. Architecture

### 2.1 Component Overview

```
┌─────────────────────────────────────────────────────┐
│                    mesh-hub binary                   │
├─────────────┬──────────────┬────────────────────────┤
│  Admin API  │  Tenant Mgr  │  Metrics / Telemetry   │
│  (HTTP/gRPC)│              │  (Prometheus + logs)    │
├─────────────┴──────────────┴────────────────────────┤
│                  Hub Logic Layer                     │
│  ┌────────────┐ ┌──────────────┐ ┌────────────────┐ │
│  │ Hub Peering│ │ Storage Mgr  │ │ Policy Engine  │ │
│  │ (BGP-style)│ │ (disk-backed)│ │ (quotas/rules) │ │
│  └────────────┘ └──────────────┘ └────────────────┘ │
├─────────────────────────────────────────────────────┤
│              Shared Protocol Layer                   │
│  ┌──────────┐ ┌───────────┐ ┌────────────────────┐  │
│  │ mesh-dht │ │ mesh-     │ │ mesh-core          │  │
│  │ (full    │ │ transport │ │ (types, crypto,    │  │
│  │ Kademlia)│ │ (QUIC)    │ │  serialization)    │  │
│  └──────────┘ └───────────┘ └────────────────────┘  │
└─────────────────────────────────────────────────────┘
```

### 2.2 Hub Logic Layer

The hub logic layer contains behavior that distinguishes a hub from a standard
node. These are not protocol changes — they are operational policies implemented
on top of the standard protocol.

**Hub Peering (Section 3)**
- Discovers other hubs via `infrastructure/hub` routing key
- Maintains a full peer table of all known hubs
- Periodic health checks and liveness probes between hubs
- Gossips hub list updates to other hubs

**Storage Manager (Section 4)**
- Disk-backed descriptor storage (not just in-memory)
- Stores descriptors beyond the hub's "responsible" DHT range
- Per-tenant storage accounting
- Eviction policies: tenant-priority, then LRU by last-queried time

**Policy Engine (Section 5)**
- Per-tenant quotas (descriptor count, total bytes, query rate)
- Rate limiting on STORE and FIND_VALUE per identity
- Tenant-aware eviction priority (paying tenants' descriptors evicted last)
- Configurable storage policies (accept-all vs. tenant-only)

---

## 3. Hub Peering

### 3.1 Hub Self-Advertisement

A hub publishes a descriptor to the mesh advertising itself:

```
schema_hash:  BLAKE3("mesh:schema:core/capability")
topic:        "hub"
type:         "infrastructure/hub"
params: {
  "capacity": {
    "max_descriptors": 10000000,
    "max_tenants": 1000,
    "storage_gb": 500
  },
  "coverage": {
    "key_space_coverage": 1.0,    // fraction of DHT key space stored
    "regions": ["us-east", "eu-west"]
  },
  "services": ["storage", "routing", "relay"],
  "hub_protocol_version": 1
}
endpoint: {
  "protocol": "mesh-negotiate",
  "address": "quic://hub.example.com:4433",
  "auth": "did-auth"
}
routing_keys: [
  BLAKE3("mesh:route:infrastructure"),
  BLAKE3("mesh:route:infrastructure/hub")
]
```

### 3.2 Hub Discovery and Peering

On startup, a hub:

1. Bootstraps into the mesh normally (Section 6 of PROTOCOL.md).
2. Performs FIND_VALUE at `BLAKE3("mesh:route:infrastructure/hub")`.
3. Collects all hub descriptors.
4. Establishes persistent QUIC connections to all discovered hubs.
5. Exchanges full hub peer lists (gossip).
6. Maintains these connections with keep-alive (PING/PONG at 10s intervals).

Hub peer list synchronization uses a simple gossip protocol:
- Each hub maintains a versioned hub list (vector clock or simple sequence).
- On connecting to a peer hub, exchange list versions.
- If peer has newer entries, pull them.
- Periodically (every 60s), push new entries to all connected hubs.
- Updates are batched: max 50 descriptor updates per gossip exchange
  (following Matrix's PDU batching pattern — prevents gossip storms).

This is NOT a new protocol message type. Hub list exchange uses the existing
STORE/FIND_VALUE mechanism — hub descriptors are just descriptors. The gossip
is an optimization: instead of querying the DHT repeatedly, hubs push updates
to each other directly over their persistent connections.

### 3.4 Hub Flags

Hubs earn flags through observed behavior (inspired by Tor's relay flag
system). Flags are tracked locally by each peer hub based on its own
observations — there is no global consensus on flags.

| Flag | Criteria | Meaning |
|------|----------|---------|
| Stable | 95%+ uptime over 7 days | Reliable for long-lived connections |
| HighCapacity | >100K descriptors stored, >100 queries/s sustained | Can handle significant load |
| FullTable | Reports key_space_coverage ≥ 0.9 | Mirrors most of the mesh |
| Verified | Operator identity confirmed out-of-band | Human-verified operator |

Flags are informational. Consumer agents and other hubs MAY use flags to
prefer certain hubs for queries or peering, but flags confer no protocol-level
privileges. A hub with no flags is treated identically at the wire level.

### 3.3 Hub Health Monitoring

Hubs track the health of peer hubs:

| Metric | Threshold | Action |
|--------|-----------|--------|
| PING latency | > 5s | Mark degraded |
| PING failure | 3 consecutive | Mark unreachable |
| Descriptor staleness | TTL expired, no refresh | Remove from hub list |
| Recovery | 3 consecutive PONG | Restore to healthy |

Unreachable hubs remain in the peer list (with status) for 24 hours before
removal, allowing for transient outages.

---

## 4. Storage Architecture

### 4.1 Three-Tier Storage

Standard mesh nodes use in-memory storage (sufficient for small descriptor
counts). Hubs use a three-tier architecture (pattern proven by IPFS pinning
services and Ethereum infrastructure providers):

| Tier | Technology | Purpose | Capacity |
|------|-----------|---------|----------|
| L1: Hot Cache | In-memory LRU (e.g., `moka` or `quick_cache`) | Frequently-queried descriptors, routing table | 1–4 GB RAM |
| L2: Warm Store | Embedded KV (`redb` recommended) | Full descriptor index, all active descriptors | 10–500 GB SSD |
| L3: Tenant DB | SQLite | Tenant accounts, identities, usage tracking, billing | Small |

Query path: L1 → L2 (on L1 miss, promote to L1). Write path: L2 (with L1
invalidation). Tenant operations: L3 only.

L1 and L2 store the same data type (descriptors). L3 is structurally
different (relational tenant data). This avoids forcing two different data
models into one engine.

```
L2 storage layout (redb):
  descriptors/
    {descriptor_id} → full serialized Descriptor

  indexes/
    by_routing_key/{routing_key} → [descriptor_id, ...]
    by_publisher/{publisher_hash} → [descriptor_id, ...]
    by_tenant/{tenant_id} → [descriptor_id, ...]
    by_schema/{schema_hash} → [descriptor_id, ...]
    by_expiry/{timestamp} → [descriptor_id, ...]

  metadata/
    stats → {total_descriptors, total_bytes, ...}
    tenant_usage/{tenant_id} → {descriptor_count, total_bytes, ...}
```

### 4.2 Storage Coverage

A standard node only stores descriptors whose routing keys fall within its
responsible DHT range. A hub stores aggressively:

- **Full coverage mode** — store ALL descriptors seen, regardless of DHT range.
  This makes the hub a complete mirror of the mesh. Useful for small/medium
  networks.
- **Selective coverage mode** — store descriptors for subscribed routing key
  prefixes. A hub might cover all of `compute/` but not `storage/`. Useful
  when the mesh grows too large for a single hub to mirror everything.

Coverage mode is hub-operator configurable. Default: full coverage.

### 4.3 Eviction Strategy

When storage capacity is reached, descriptors are evicted in this priority
order (lowest priority evicted first):

1. Expired descriptors (TTL passed) — always evicted immediately
2. Descriptors with no tenant association (unclaimed)
3. Free-tier tenant descriptors, ordered by last-queried time (LRU)
4. Paid-tier tenant descriptors, ordered by last-queried time (LRU)
5. Hub infrastructure descriptors (`infrastructure/*`) — evicted last

A background task runs every 60 seconds to clean expired descriptors.
Eviction under capacity pressure is triggered synchronously on STORE when
storage is full.

---

## 5. Multi-Tenant Architecture

### 5.1 Tenant Model

A tenant is a hub-level concept — it does not exist in the mesh protocol.

```
Tenant {
  id:             uuid
  name:           string
  tier:           "free" | "pro" | "enterprise"
  identities:     [Identity]        // list of publisher DIDs belonging to this tenant
  created_at:     timestamp

  // Quotas (per tier)
  max_descriptors:    u64
  max_total_bytes:    u64
  max_query_rate:     u32           // FIND_VALUE requests per second
  max_store_rate:     u32           // STORE requests per second

  // Usage tracking
  current_descriptors: u64
  current_bytes:       u64

  // Billing
  billing_id:     string?           // external billing system reference
  billing_status: "active" | "suspended" | "trial"
  mu_balance:     i64               // remaining Mesh Units (see Section 5.5)
}
```

### 5.2 Identity Registration

When a tenant registers a new publisher identity with the hub:

1. Tenant provides the public key (Identity) via the admin API.
2. Hub generates a challenge (random nonce).
3. Tenant signs the challenge with the corresponding private key.
4. Hub verifies the signature, confirming ownership.
5. Identity is added to the tenant's identity list.

This proves the tenant controls the private key without the hub ever seeing it.

### 5.3 Tenant Isolation

Tenants are isolated at the storage and policy layer, NOT at the protocol
layer. All descriptors enter through the same QUIC transport and go through
the same protocol validation. After validation:

- The hub checks if the descriptor's `publisher` matches any registered
  tenant identity.
- If matched: descriptor is tagged with the tenant ID, counted against
  their quota, and stored with tenant-priority eviction.
- If unmatched: descriptor is stored as "unclaimed" with lowest eviction
  priority. This maintains the hub's role as a public DHT participant —
  it still stores other agents' descriptors, just with lower priority.

### 5.4 Default Tier Quotas

| Tier | Max Descriptors | Max Storage | Query Rate | Store Rate |
|------|----------------|-------------|------------|------------|
| Free | 100 | 1 MB | 10/s | 1/s |
| Pro | 10,000 | 100 MB | 100/s | 10/s |
| Enterprise | 1,000,000 | 10 GB | 1,000/s | 100/s |

Quotas are hub-operator configurable. These are recommended defaults.

### 5.5 Mesh Units (Billing Model)

All hub operations are normalized to a single billing primitive: the **Mesh
Unit (MU)**. This follows Alchemy's Compute Unit pattern, which is proven at
massive scale for metering heterogeneous operations through a single metric.

| Operation | Cost | Rationale |
|-----------|------|-----------|
| FIND_VALUE (query) | 1 MU | Lightweight read |
| FIND_NODE (routing) | 1 MU | Lightweight read |
| STORE (publish) | 10 MU | Write + index + replication |
| STORE (republish, same topic) | 5 MU | Update, less work than new publish |
| Descriptor retention | 1 MU/day per descriptor | Ongoing storage cost |
| Cross-hub routed query | 5 MU | Hub forwards to peer hub on behalf of tenant |

MU costs are hub-operator configurable. These are recommended defaults.

**Metering:** Every protocol operation that enters the hub is tagged with a
tenant ID (matched by publisher identity) and its MU cost is deducted from
the tenant's balance. Operations from unregistered identities are metered
against a global "unclaimed" budget with the lowest priority.

**Quota enforcement:**
- Webhook notification at 80% MU budget consumed
- Soft limit at 100%: new STORE requests rejected, queries still served
- Hard limit at 120%: all operations rejected until balance is replenished
- Free tier: MU budget resets monthly. Paid tiers: usage-based billing.

**Billing integration:** The hub tracks MU consumption per tenant and exposes
usage via the admin API (`GET /api/v1/account/usage`). Actual payment
processing is external — the hub emits usage events (webhook or log) that
feed into whatever billing system the operator chooses (Stripe, custom, etc.).
The hub does not process payments directly.

---

## 6. Admin API

### 6.1 API Design

The admin API is an HTTP REST API (not a mesh protocol feature) for hub
operators and tenants to manage the hub.

**Operator endpoints** (authenticated with operator key):

```
GET    /api/v1/hub/status          — hub health, peer count, storage stats
GET    /api/v1/hub/peers           — full hub peer list with health status
GET    /api/v1/hub/metrics         — Prometheus-format metrics
POST   /api/v1/tenants             — create tenant
GET    /api/v1/tenants             — list tenants
GET    /api/v1/tenants/{id}        — tenant details + usage
PUT    /api/v1/tenants/{id}        — update tenant (tier, quotas)
DELETE /api/v1/tenants/{id}        — deactivate tenant
```

**Tenant endpoints** (authenticated with tenant DID):

```
GET    /api/v1/account              — tenant's own account details + usage
POST   /api/v1/account/identities   — register a new publisher identity
DELETE /api/v1/account/identities/{did} — remove an identity
GET    /api/v1/account/descriptors   — list tenant's stored descriptors
GET    /api/v1/account/usage         — usage stats and quota status
```

### 6.2 Authentication

- **Operator authentication:** bearer token or mTLS. Hub-operator configured.
- **Tenant authentication:** DID-auth challenge-response. Tenant signs a
  challenge with one of their registered identity keys. This keeps auth
  consistent with the mesh protocol's identity model.

---

## 7. Observability

### 7.1 Metrics (Prometheus)

```
# DHT
mesh_hub_routing_table_size          — entries in routing table
mesh_hub_routing_table_buckets_used  — non-empty k-buckets
mesh_hub_dht_queries_total           — FIND_VALUE/FIND_NODE requests handled
mesh_hub_dht_stores_total            — STORE requests handled
mesh_hub_dht_query_latency_seconds   — histogram

# Storage
mesh_hub_descriptors_total           — total stored descriptors
mesh_hub_storage_bytes_total         — total storage used
mesh_hub_evictions_total             — eviction events by reason
mesh_hub_expired_cleanups_total      — expired descriptor removals

# Hub peering
mesh_hub_peers_total                 — known hub peers
mesh_hub_peers_healthy               — healthy hub peers
mesh_hub_peers_degraded              — degraded hub peers
mesh_hub_peer_gossip_total           — hub list gossip exchanges

# Tenants
mesh_hub_tenants_total               — registered tenants by tier
mesh_hub_tenant_descriptors          — descriptors per tenant (labeled)
mesh_hub_tenant_quota_usage_ratio    — quota utilization (0.0–1.0)
mesh_hub_tenant_rate_limited_total   — rate-limited requests per tenant

# Transport
mesh_hub_quic_connections_active     — active QUIC connections
mesh_hub_quic_streams_total          — streams opened
mesh_hub_bandwidth_bytes_total       — bytes in/out
```

### 7.2 Structured Logging

All hub operations emit structured JSON logs with fields:
- `event` — what happened
- `tenant_id` — if applicable
- `publisher` — DID of the publisher
- `descriptor_id` — if applicable
- `peer` — if hub peering related
- `duration_ms` — operation latency

### 7.3 Health Check

```
GET /healthz          — returns 200 if hub is operational
GET /readyz           — returns 200 if hub is ready to serve (routing table populated, storage online)
```

---

## 8. Deployment

### 8.1 Minimum Requirements

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| CPU | 2 cores | 4+ cores |
| RAM | 2 GB | 8+ GB |
| Storage | 10 GB SSD | 100+ GB SSD |
| Network | 100 Mbps, static IP | 1 Gbps, static IP |
| Ports | 4433/UDP (QUIC), 8080/TCP (admin API) | same |

### 8.2 Configuration

Hub configuration via TOML file:

```toml
[identity]
keypair_path = "/etc/mesh-hub/keypair.bin"

[network]
listen_addr = "0.0.0.0:4433"
admin_addr = "127.0.0.1:8080"

[dht]
bucket_size = 20            # K parameter
concurrency = 3             # α parameter
bucket_refresh_interval = "1h"

[storage]
engine = "redb"             # "redb" | "rocksdb" | "sled"
data_dir = "/var/lib/mesh-hub/data"
max_capacity_gb = 100
coverage_mode = "full"      # "full" | "selective"
hot_cache_mb = 2048         # L1 in-memory LRU cache size
tenant_db = "/var/lib/mesh-hub/tenants.sqlite"

[hub_peering]
enabled = true
gossip_interval = "60s"
health_check_interval = "10s"
unreachable_retention = "24h"

[tenants]
enabled = true
default_tier = "free"

[tenants.tiers.free]
max_descriptors = 100
max_storage_bytes = 1048576
max_query_rate = 10
max_store_rate = 1

[tenants.tiers.pro]
max_descriptors = 10000
max_storage_bytes = 104857600
max_query_rate = 100
max_store_rate = 10

[tenants.tiers.enterprise]
max_descriptors = 1000000
max_storage_bytes = 10737418240
max_query_rate = 1000
max_store_rate = 100

[policy]
store_mode = "open"         # "open" | "tenant-only" | "allowlist"
blocked_identities = []
blocked_types = []
blocked_routing_keys = []

[security]
max_connections_per_ip = 50
max_new_connections_per_ip_per_sec = 10
max_queries_per_identity_per_sec = 20
invalid_submission_cooldown = "1h"
admin_tls_required = true   # require TLS for non-localhost admin API

[telemetry]
metrics_enabled = true
log_format = "json"         # "json" | "text"
log_level = "info"
```

---

## 9. Security

### 9.1 Transport-Level Protection

A hub is a publicly-reachable, always-on target. These protections are
baseline operational requirements, not optional hardening.

**Connection rate limiting:**
- Max new QUIC connections per source IP per second: configurable, default 10.
- Max concurrent QUIC connections per source IP: configurable, default 50.
- Connections exceeding limits are silently dropped (no error response —
  avoid amplification).

**QUIC amplification mitigation:**
- QUIC's built-in address validation (retry tokens) MUST be enabled.
  This prevents IP-spoofed amplification attacks where an attacker sends
  small requests with a forged source IP to generate large responses
  directed at the victim.
- Hub MUST NOT send responses larger than the request until the client's
  address is validated via QUIC retry.

**Protocol abuse:**
- Max FIND_VALUE requests per source identity per second: configurable,
  default 20. Exceeding this returns STORE_ACK with `stored: false` and
  the connection is deprioritized (not dropped — avoid signaling the limit
  to attackers).
- Max STORE requests per source identity per minute: follows Section 9.3
  of PROTOCOL.md (recommended 10 per identity per minute).
- Descriptors that fail validation (Section 2.2 of PROTOCOL.md) increment
  a per-identity penalty counter. Identities exceeding 10 invalid
  submissions per minute are temporarily blocked (1 hour cooldown).

### 9.2 Admin API Lockdown

The admin API MUST be secure by default:

- **Bind to localhost only** (`127.0.0.1:8080`). Operators who need remote
  access MUST explicitly configure an allowlist or use a reverse proxy.
- **Operator authentication required** on all operator endpoints. No
  unauthenticated admin access, even from localhost.
- **Tenant endpoints** are authenticated via DID-auth (Section 6.2).
  Failed auth attempts are rate-limited: max 5 failures per source IP
  per minute.
- **TLS required** for any non-localhost admin API access. The hub SHOULD
  refuse to start if admin API is bound to a public address without TLS
  configured.

### 9.3 Operator Controls (Abuse Handling)

Hub operators need tools to handle abuse regardless of jurisdiction. The
protocol is neutral; the hub is not — it's operated by a legal entity.

**Blocklists:**

```toml
[policy]
# Block specific publisher identities (by DID)
blocked_identities = [
  "did:mesh:z6Mk...",
]

# Block capability type patterns (glob syntax)
blocked_types = [
  "illegal/*",
  "*/exploit/*",
]

# Block descriptors containing specific routing keys
blocked_routing_keys = []
```

Blocked descriptors are rejected at STORE time with `stored: false,
reason: "policy"`. The hub does not reveal which policy triggered the
rejection.

**Allowlist mode:**
For restricted hubs (private mesh, enterprise), an operator can enable
allowlist-only mode where ONLY registered tenant identities can STORE.
Unregistered identities can still query (FIND_VALUE) but not publish.

```toml
[policy]
store_mode = "open"         # "open" | "tenant-only" | "allowlist"
```

---

## 10. Operational Lifecycle

### 10.1 Graceful Shutdown

A hub shutdown (for upgrades, maintenance, or migration) MUST minimize
disruption:

1. **Announce unavailability** — publish an updated hub descriptor with
   `constraints.temporal.available: false` and a reduced TTL (60s).
   This signals peer hubs and clients to stop routing new requests here.
2. **Drain period** — stop accepting new QUIC connections. Continue
   processing requests on existing connections for a configurable drain
   period (default 30s).
3. **Peer notification** — send PING with a "shutting down" flag (using
   the existing PING/PONG — the flag is informational, peers that don't
   understand it simply see a normal PING). Peer hubs mark this hub as
   unreachable immediately rather than waiting for 3 failed PINGs.
4. **Flush state** — flush in-memory MU balance decrements to SQLite.
   Flush any pending storage writes to redb.
5. **Close connections** — close all QUIC connections. Shut down.

On restart, the hub re-advertises itself and peer hubs restore it to
healthy status after 3 successful PONGs (Section 3.3).

### 10.2 Backup and Recovery

Hub operators SHOULD maintain backups of persistent state:

- **L2 (redb)** — periodic snapshots. Recommended: every 6 hours. Redb
  supports consistent snapshots without stopping the hub.
- **L3 (SQLite)** — periodic backups. Recommended: every hour. SQLite
  supports online backup via `.backup` command.
- **Keypair** — the hub's identity keypair MUST be backed up securely and
  separately. Loss of the keypair means loss of the hub's identity — it
  would rejoin the mesh as a new node with a new DID.

**Recovery from data loss:**
- If L2 is lost: the hub rejoins the mesh with an empty descriptor store.
  It repopulates by receiving STOREs from the DHT (other nodes still hold
  the descriptors). Tenant-pinned descriptors are recovered when tenants
  republish. Recovery time depends on mesh size and traffic — for a
  mid-size network, expect full repopulation within 2-4 TTL cycles.
- If L3 is lost: tenant accounts and identity mappings are gone. Tenants
  must re-register. This is the more critical backup target.
- If keypair is lost: the hub must generate a new identity. All peer
  relationships and tenant trust anchors reset. This is catastrophic —
  keypair backup is essential.

### 10.3 Migration from mesh-node

An operator running a standard `mesh-node` can graduate to a `mesh-hub`:

1. Stop `mesh-node`.
2. Install `mesh-hub` binary (same workspace, shared crate dependencies).
3. Create a hub config file (Section 8.2). The existing keypair is reused —
   the node keeps its identity and DID.
4. Start `mesh-hub`. It initializes L2 storage (redb) and L3 tenant
   database (SQLite) on first run.
5. The hub bootstraps normally, expands its routing table to full 256
   buckets, and begins storing descriptors beyond its responsible DHT range.
6. Once stable, the hub publishes its `infrastructure/hub` descriptor and
   begins peering.

No data migration is needed. The hub starts with an empty descriptor store
and populates it from the DHT. In-memory state from the old `mesh-node`
is not transferable, but repopulation is fast — the routing table rebuilds
within a few FIND_NODE rounds, and descriptors flow in via normal DHT
replication.

---

## 11. Build Phases

### Pre-Phase: Upstream Refactors (status: complete)

These changes go into the existing mesh-protocol crates at
`/home/logan/Dev/mesh-protocol`. They must land BEFORE mesh-hub
development begins — mesh-hub depends on them.

- [x] **Extract `DescriptorStorage` trait from `DescriptorStore`** (mesh-dht)
  - Define the trait in `mesh-dht/src/storage.rs`
  - Make existing `DescriptorStore` implement the trait (in-memory default)
  - Make `DhtNode` generic: `DhtNode<S: DescriptorStorage>`
  - Update mesh-node to use `DhtNode<DescriptorStore>` (no behavior change)
  - All existing tests must pass unchanged
- [x] **Add `ProtocolHook` trait** (mesh-dht)
  - Define in `mesh-dht/src/hooks.rs`
  - Default implementation is no-op (pass-through)
  - Wire into `DhtNode` message handlers (pre_store, post_store,
    pre_query, post_query)
  - `DhtNode` accepts `Option<Arc<dyn ProtocolHook>>`
  - mesh-node passes `None` (no behavior change)
- [x] **Expose mesh-dht public API for downstream composition**
  - Ensure `DhtNode`, `DescriptorStorage`, `ProtocolHook`, `RoutingTable`,
    `DhtConfig` are all `pub` in `mesh-dht/src/lib.rs`
  - Ensure `Transport` trait is `pub` (already is)
- [x] **Create `mesh-schemas` crate** (mesh-schemas)
  - Crate scaffolding in workspace
  - Well-known schema hash constants (`core/capability`, `infrastructure/hub`, `core/relay`)
  - Routing key derivation helpers for well-known infrastructure types
  - Scope: protocol-level infrastructure schemas ONLY — application-specific
    capability schemas are emergent and do not belong here
- [x] **Create `mesh-client` crate** (mesh-client)
  - Crate scaffolding in workspace
  - High-level `MeshClient` API: connect → publish → discover → resolve
  - Connection management (bootstrap node list, automatic reconnection)
  - Descriptor builder API for ergonomic descriptor construction
  - Integration tests against mesh-node

### Phase 0: Core Hub (status: implemented)
- [x] `mesh-hub` crate scaffolding in workspace
- [x] `mesh-hub/src/lib.rs` — HubRuntime with builder pattern (Section 1.3)
- [x] `mesh-hub/src/main.rs` — thin binary wrapper over HubRuntime
- [x] Full 256-bucket Kademlia routing table (already in mesh-dht)
- [x] Implement `DescriptorStorage` trait with redb backend (disk-backed)
- [x] L1 in-memory hot cache wrapping redb storage (moka)
- [x] Storage indexes (by routing key, dedup key, sequence tracking)
- [x] Expiry background task (60s interval)
- [ ] Hub self-advertisement (`infrastructure/hub` descriptor) — deferred to Phase 1
- [x] TenantManager with SQLite backend (public API, Section 1.3)
- [x] Admin API as composable Axum Router (public, mergeable by downstream)
- [x] CLI binary with TOML config
- [x] Health check endpoints (/healthz, /readyz)
- [ ] Connection rate limiting per IP (Section 9.1) — deferred to Phase 4
- [ ] Protocol abuse rate limiting per identity (Section 9.1) — deferred to Phase 4
- [ ] Admin API localhost-only binding with auth (Section 9.2) — deferred to Phase 2
- [x] Policy engine — blocklists and store mode (Section 9.3)
- [x] Graceful shutdown with drain period (Section 10.1)

### Phase 1: Hub Peering (status: planned)
- [ ] Hub discovery via FIND_VALUE at `infrastructure/hub` routing key
- [ ] Persistent QUIC connections to peer hubs
- [ ] Hub list gossip protocol (batched, max 50 per exchange)
- [ ] Peer health monitoring and status tracking
- [ ] Hub flag tracking (Stable, HighCapacity, FullTable, Verified)

### Phase 2: Multi-Tenant (status: planned)
- [ ] L3 tenant database (SQLite)
- [ ] Tenant data model and persistence
- [ ] Identity registration with challenge-response verification
- [ ] Per-tenant descriptor tagging and quota tracking
- [ ] Mesh Unit metering and budget enforcement
- [ ] Tenant-aware eviction policies
- [ ] Rate limiting per identity and per tenant
- [ ] Admin API — operator endpoints
- [ ] Admin API — tenant endpoints with DID-auth
- [ ] Usage webhook emission for external billing

### Phase 3: Observability (status: planned)
- [ ] Prometheus metrics exporter
- [ ] Structured JSON logging
- [ ] Per-tenant usage dashboards
- [ ] Alerting hooks (webhook-based)

### Phase 4: Hardening (status: planned)
- [ ] Load testing — 100K descriptors, 1K tenants, sustained query load
- [ ] Eviction pressure testing
- [ ] Hub peering partition/recovery testing
- [ ] Security audit — admin API auth, tenant isolation, rate limiting
- [ ] Backup/recovery testing (L2 loss, L3 loss, keypair recovery)
- [ ] Graceful shutdown testing under load
- [ ] Documentation — operator guide, tenant onboarding guide, migration guide

---

## 12. Resolved Design Questions

1. **Tenant persistence** — SQLite (L3) for tenant/account data, redb (L2) for
   descriptors. Different data models belong in different engines. SQLite gives
   relational queries for tenant management; redb gives fast KV access for
   descriptor storage. Both are embedded, no external dependencies.

2. **Billing integration** — Mesh Units (Section 5.5) normalize all operations
   to a single metric. The hub tracks MU consumption internally and exposes
   usage via API/webhooks. Actual payment processing is external and
   operator-chosen. This keeps the hub focused on metering, not payments.

3. **Hub-to-hub protocol** — existing STORE/FIND_VALUE is sufficient. Hub
   gossip uses the same descriptor primitives with batching (max 50 updates
   per exchange, Section 3.2). A dedicated gossip channel is **not needed in
   this specification** — the overhead of descriptor-based gossip is negligible
   at hub counts of tens of thousands. If the hub network grows to a scale
   where gossip efficiency matters, a dedicated QUIC stream-based gossip
   protocol can be added without protocol changes (it would operate alongside
   the existing 8 message types on the same QUIC connection).
