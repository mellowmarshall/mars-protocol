# Mesh Hub Operator Guide

## Overview

A mesh hub is a high-capacity node in the Capability Mesh Protocol network. It speaks the same wire protocol as any mesh node but operates at a higher tier:

- **Disk-backed storage** (redb) with an LRU hot cache
- **Full Kademlia routing tables** for efficient descriptor lookups
- **Multi-tenant management** with SQLite-backed identity registration and MU metering
- **Admin API** (Axum HTTP) for tenant CRUD, usage monitoring, and metrics
- **Hub-to-hub peering** with gossip-based descriptor replication and health checks

`mesh-hub` is a **library-first** crate. The `mesh-hub` binary is a thin CLI wrapper over `HubRuntime`. Downstream projects can import `mesh-hub` as a library, call `HubRuntime::builder(config, keypair).build()`, merge additional routes onto the admin router, and run the hub embedded in a larger application.

## Installation

```bash
cargo build --release
```

Binaries are placed in `target/release/`:
- `mesh-hub` -- hub node
- `mesh-node` -- lightweight mesh node / CLI tool

## Configuration Reference

The hub reads a TOML configuration file (default: `mesh-hub.toml`).

```bash
mesh-hub --config /path/to/mesh-hub.toml
```

### Minimal Configuration

Only `identity.keypair_path` is required. Everything else has defaults.

```toml
[identity]
keypair_path = "data/hub.key"
```

### Full Configuration

```toml
# ── Identity ──────────────────────────────────────────────
[identity]
keypair_path = "data/hub.key"     # Path to 32-byte Ed25519 secret key

# ── Network ───────────────────────────────────────────────
[network]
listen_addr = "0.0.0.0:4433"     # QUIC protocol listener (default)
admin_addr = "127.0.0.1:8080"    # Admin API HTTP listener (default)

# ── Storage ───────────────────────────────────────────────
[storage]
data_dir = "data"                 # Directory for redb descriptor store (default)
hot_cache_entries = 10000         # LRU hot cache size (default: 10000)

# ── Tenants ───────────────────────────────────────────────
[tenants]
enabled = true                    # Enable tenant management (default: true)
db_path = "data/tenants.sqlite"   # SQLite database path (default)

# ── Policy ────────────────────────────────────────────────
[policy]
store_mode = "open"               # "open" | "tenant-only" | "allowlist" (default: "open")
blocked_identities = []           # DIDs blocked from STORE operations
blocked_routing_keys = []         # Hex-encoded routing key digests to reject

# ── Security ──────────────────────────────────────────────
[security]
max_connections_per_ip = 50       # Per-IP connection cap (default: 50)
max_queries_per_identity_per_sec = 20  # Per-identity query cap (default: 20)
admin_bearer_token = ""           # Optional bearer token for admin API (deprecated alias)
outbound_allowlist = []           # Socket addresses allowed for outbound even if private/loopback

# ── Peering ───────────────────────────────────────────────
[peering]
enabled = false                   # Enable hub-to-hub peering (default: false)
gossip_interval_secs = 60         # Seconds between gossip rounds (default: 60)
health_check_interval_secs = 10   # Seconds between health checks (default: 10)
max_peers = 50                    # Maximum connected peer hubs (default: 50)
regions = []                      # Region tags for metadata advertisement
max_descriptors = 1000000         # Advertised descriptor capacity (default: 1000000)

# ── MU Costs ──────────────────────────────────────────────
[mu_costs]
store_new = 10                    # MU cost: new descriptor store (default: 10)
store_update = 5                  # MU cost: update existing descriptor (default: 5)
find_value = 1                    # MU cost: FIND_VALUE query (default: 1)
find_node = 1                     # MU cost: FIND_NODE query (default: 1)

# ── Observability ─────────────────────────────────────────
[observability]
metrics_enabled = true            # Enable Prometheus /metrics endpoint (default: true)

# ── Operator Token ────────────────────────────────────────
operator_token = "your-secret-token"  # Bearer token for operator-only admin API endpoints
```

### Store Modes

| Mode | Behavior |
|------|----------|
| `open` | Any identity can STORE descriptors (default) |
| `tenant-only` | Only registered tenant identities can STORE |
| `allowlist` | Same as `tenant-only` -- only registered tenant identities can STORE |

### Rate Limiting (Built-in Defaults)

Rate limiting is not TOML-configurable. It uses hardcoded sliding-window defaults:

| Operation | Per-IP Limit | Per-Identity Limit | Window |
|-----------|-------------|-------------------|--------|
| Connect   | 60/min      | --                | 60s    |
| Store     | 30/min      | 20/min            | 60s    |
| Query     | 300/min     | 200/min           | 60s    |

Stale rate-limit entries are cleaned every 2 minutes.

## Admin API Reference

The admin API is served on the `admin_addr` (default `127.0.0.1:8080`).

Operator-only endpoints require `Authorization: Bearer <operator_token>` when `operator_token` is configured. If no token is set, all endpoints are open (not recommended for production).

### Health Checks

#### `GET /healthz`
**Auth:** None

Returns `200 OK` unconditionally. Use for liveness probes.

#### `GET /readyz`
**Auth:** None

Returns `200 OK` if the descriptor storage is accessible, `503 Service Unavailable` otherwise. Use for readiness probes.

### Hub Status

#### `GET /api/v1/hub/status`
**Auth:** None

Returns hub runtime status.

```json
{
  "uptime_secs": 3600,
  "descriptor_count": 1234,
  "routing_key_count": 567,
  "tenant_count": 3
}
```

### Tenant Management

#### `GET /api/v1/tenants`
**Auth:** Operator

List all tenants.

```json
[
  {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "my-org",
    "tier": "free",
    "max_descriptors": 100,
    "max_storage_bytes": 1048576,
    "max_query_rate": 10,
    "max_store_rate": 1,
    "current_descriptors": 0,
    "current_bytes": 0,
    "mu_balance": 10000,
    "mu_limit": 10000,
    "created_at": 1711152000000000
  }
]
```

#### `POST /api/v1/tenants`
**Auth:** Operator

Create a new tenant.

**Request:**
```json
{
  "name": "my-org",
  "tier": "free"
}
```

`tier` defaults to `"free"` if omitted. Valid tiers: `free`, `starter`, `pro`, `enterprise`.

**Response:** `201 Created` with the tenant object.

#### `GET /api/v1/tenants/:id`
**Auth:** Operator

Get a tenant by UUID. Returns `404` if not found.

#### `DELETE /api/v1/tenants/:id`
**Auth:** Operator

Delete a tenant and all its registered identities. Returns `204 No Content` on success, `404` if not found.

### Identity Registration (Operator)

#### `POST /api/v1/tenants/:id/identities`
**Auth:** Operator

Directly register an identity to a tenant (bypasses challenge-response).

**Request:**
```json
{
  "did": "did:mesh:z6Mk...",
  "identity_bytes": "01abcdef..."
}
```

`identity_bytes` is hex-encoded (algorithm byte + public key).

**Response:** `201 Created`

#### `DELETE /api/v1/tenants/:id/identities/:did`
**Auth:** Operator

Remove an identity from a tenant. Returns `204 No Content` on success.

### Identity Registration (Challenge-Response)

These endpoints are public -- they implement the DID-Auth challenge-response flow so that nodes can self-register without operator intervention.

#### `POST /api/v1/tenants/:id/identities/challenge`
**Auth:** None

Request a challenge for identity verification.

**Request:**
```json
{
  "action": "register"
}
```

**Response:** `201 Created`
```json
{
  "id": "challenge-uuid",
  "nonce": "hex-encoded-32-bytes",
  "hub_did": "did:mesh:z6Mk...",
  "action": "register",
  "issued_at": 1711152000000000,
  "expiry": 1711152300000000
}
```

Challenges expire after 5 minutes and can only be used once.

#### `POST /api/v1/tenants/:id/identities/verify`
**Auth:** None

Submit a signed challenge to verify identity and register it.

**Request:**
```json
{
  "challenge_id": "challenge-uuid",
  "identity_bytes": "01abcdef...",
  "did": "did:mesh:z6Mk...",
  "signature": "hex-encoded-signature"
}
```

The signature must be over the challenge's canonical signable bytes (CBOR-encoded: action, nonce, hub_did, issued_at).

**Response:** `200 OK`
```json
{
  "status": "registered",
  "did": "did:mesh:z6Mk..."
}
```

**Error codes:** `403` invalid signature, `404` challenge not found, `410` challenge expired.

### Usage and Quotas

#### `GET /api/v1/tenants/:id/usage`
**Auth:** Operator

Get current usage for a tenant.

```json
{
  "current_descriptors": 42,
  "current_bytes": 102400,
  "mu_balance": 8500,
  "mu_limit": 10000
}
```

#### `PATCH /api/v1/tenants/:id/quota`
**Auth:** Operator

Update tenant quota limits. All fields are optional -- only provided fields are updated.

**Request:**
```json
{
  "max_descriptors": 500,
  "max_storage_bytes": 5000000,
  "mu_limit": 50000
}
```

**Response:** `200 OK` with the updated tenant object.

### Metrics

#### `GET /metrics`
**Auth:** Operator

Prometheus text exposition format. See [Monitoring](#monitoring) for metric details.

## Tenant Management

### Tiers and Quotas

| Tier | Max Descriptors | Max Storage | Query Rate | Store Rate | MU Limit |
|------|----------------|-------------|------------|------------|----------|
| free | 100 | 1 MB | 10/s | 1/s | 10,000 |
| starter | 1,000 | 10 MB | 50/s | 5/s | 100,000 |
| pro | 10,000 | 100 MB | 100/s | 10/s | 1,000,000 |
| enterprise | 1,000,000 | 10 GB | 1,000/s | 100/s | 10,000,000 |

MU (Metering Units) are deducted per operation according to `[mu_costs]`. New tenants start with `mu_balance == mu_limit`. Quotas can be adjusted per-tenant via `PATCH /api/v1/tenants/:id/quota`.

### Identity Registration Flow

Tenants need registered identities to publish descriptors (when `store_mode` is `tenant-only` or `allowlist`).

**Operator registration** (direct):
```bash
curl -X POST http://localhost:8080/api/v1/tenants/$TENANT_ID/identities \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"did": "did:mesh:z6Mk...", "identity_bytes": "01abcdef..."}'
```

**Self-registration** (challenge-response):

1. Node requests a challenge:
   ```bash
   curl -X POST http://localhost:8080/api/v1/tenants/$TENANT_ID/identities/challenge \
     -H "Content-Type: application/json" \
     -d '{"action": "register"}'
   ```

2. Node signs the challenge's canonical bytes with its private key.

3. Node submits the signed response:
   ```bash
   curl -X POST http://localhost:8080/api/v1/tenants/$TENANT_ID/identities/verify \
     -H "Content-Type: application/json" \
     -d '{
       "challenge_id": "...",
       "identity_bytes": "...",
       "did": "did:mesh:z6Mk...",
       "signature": "..."
     }'
   ```

## Monitoring

### Prometheus Metrics

All metrics are aggregate only -- no tenant_id or publisher DID labels are exposed (Security #8: Metadata Leak Prevention).

| Metric | Type | Description |
|--------|------|-------------|
| `mesh_hub_stores_total` | Counter | Total STORE operations processed |
| `mesh_hub_queries_total` | Counter | Total FIND_VALUE queries processed |
| `mesh_hub_store_duration_seconds` | Histogram | STORE processing latency |
| `mesh_hub_query_duration_seconds` | Histogram | FIND_VALUE processing latency |
| `mesh_hub_descriptors_total` | Gauge | Current descriptor count |
| `mesh_hub_evictions_total` | Counter | Descriptors evicted (expiry) |
| `mesh_hub_peers_connected` | Gauge | Connected peer hubs |
| `mesh_hub_gossip_rounds_total` | Counter | Gossip rounds completed |
| `mesh_hub_rate_limited_total` | Counter | Requests rejected by rate limiter (label: `operation`) |
| `mesh_hub_connections_active` | Gauge | Active QUIC connections |

### Scrape Configuration

```yaml
# prometheus.yml
scrape_configs:
  - job_name: mesh-hub
    scheme: http
    authorization:
      type: Bearer
      credentials: your-secret-token
    static_configs:
      - targets: ["localhost:8080"]
```

### Key Metrics to Alert On

- **`mesh_hub_rate_limited_total`** -- sustained increase indicates abuse or misconfigured clients
- **`mesh_hub_descriptors_total`** approaching storage capacity
- **`mesh_hub_peers_connected`** dropping to 0 (peering failure)
- **`mesh_hub_store_duration_seconds`** p99 exceeding 1s (storage bottleneck)
- **`mesh_hub_evictions_total`** high rate may indicate TTL misconfiguration

## Security Hardening Checklist

- [ ] **Set `operator_token`** -- required for production. Without it, all admin API endpoints are open.
- [ ] **Use `tenant-only` or `allowlist` store mode** -- prevents anonymous STORE operations.
- [ ] **Configure `outbound_allowlist`** for private deployments -- the hub rejects outbound connections to private/loopback/RFC1918 addresses by default (SSRF prevention). Add specific addresses if hubs need to peer on private networks.
- [ ] **Monitor `rate_limited_total` metric** -- track rate-limited requests to detect abuse.
- [ ] **Restrict admin API to localhost or internal network** -- the default `admin_addr` of `127.0.0.1:8080` binds to localhost only. Do not expose to the public internet.
- [ ] **Rotate hub keypair periodically** -- generate a new keypair with `mesh-hub --generate-keypair`, update `keypair_path`, and restart. The old DID will no longer be valid for challenge generation.
- [ ] **Set restrictive file permissions on keypair** -- the binary writes keys with mode `0600`. Verify permissions if keys are provisioned externally.
- [ ] **Review `blocked_identities` and `blocked_routing_keys`** -- maintain blocklists for known-bad actors or content.

## Hub Peering

When `peering.enabled = true`, hubs discover each other and replicate descriptors through a gossip protocol.

### How It Works

1. **Self-advertisement:** On startup (and every 30 minutes), the hub publishes a self-advertisement descriptor to its local DHT using the `infrastructure/hub` schema and routing key. The payload includes the hub's endpoint address, max descriptor capacity, and region tags.

2. **Discovery:** Every 5 minutes, the hub queries the DHT for other hub advertisements under the `infrastructure/hub` routing key. Discovered hubs are added to the peer manager.

3. **Gossip:** Every `gossip_interval_secs` (default 60s), the hub sends a batch of up to 50 descriptors to each connected peer via STORE messages. Peers reciprocate.

4. **Health checks:** Every `health_check_interval_secs` (default 10s), the hub pings each peer. After 3 consecutive failures a peer is marked unhealthy; after 5 it is disconnected.

5. **SSRF prevention:** All outbound peer connections are validated against the SSRF filter. Private/loopback addresses are rejected unless they appear in `security.outbound_allowlist`.

### Configuration Example

```toml
[peering]
enabled = true
gossip_interval_secs = 30
health_check_interval_secs = 10
max_peers = 20
regions = ["us-east", "us-west"]
max_descriptors = 500000

[security]
outbound_allowlist = ["10.0.1.5:4433", "10.0.1.6:4433"]
```

### Peering Topology

Hubs form a loosely-connected overlay. There is no leader election or consensus -- each hub independently discovers peers and replicates descriptors via gossip. Convergence depends on gossip interval and peer count. For reliable replication across a cluster, ensure each hub can reach at least 2-3 peers.
