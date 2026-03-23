# PLAN-02: PurposeBot Hub — Private Implementation

**Status:** In Design
**Date:** 2026-03-22
**Depends on:** PROTOCOL.md, PLAN-01-mesh-hub.md, hub-payment-implementation.md

---

## 1. Overview

The PurposeBot Hub is a commercial capability mesh hub operated by PurposeBot.
It is built on top of the open source `mesh-protocol` crates and the
reference `mesh-hub` binary, adding payment, multi-tenant SaaS features,
a web portal, delegated routing, and descriptor replication.

This is NOT a fork. The open source project is a dependency. Private code
lives in a separate repository and imports `mesh-*` crates.

### 1.1 Repo Boundary

```
github.com/purposebot/mesh-protocol (public)
├── mesh-core/          — types, serialization, hashing, signing
├── mesh-dht/           — Kademlia implementation
├── mesh-transport/     — QUIC transport
├── mesh-node/          — standard node binary
├── mesh-hub/           — reference hub binary (open source)
├── mesh-client/        — lightweight client library
├── mesh-schemas/       — core schema definitions
└── PROTOCOL.md         — the spec

github.com/purposebot/purposebot-hub (private)
├── src/
│   ├── main.rs              — PurposeBot hub binary (wraps mesh-hub)
│   ├── config.rs            — extended config (Stripe, portal, etc.)
│   ├── payment/
│   │   ├── mod.rs
│   │   ├── stripe.rs        — Stripe integration
│   │   ├── metering.rs      — MU tracking and enforcement
│   │   ├── trials.rs        — trial account management
│   │   └── webhooks.rs      — Stripe webhook handlers
│   ├── portal/
│   │   ├── mod.rs
│   │   ├── routes.rs        — web portal HTTP routes
│   │   ├── auth.rs          — session auth (portal → admin API)
│   │   └── templates/       — HTML templates (Askama or Tera)
│   ├── delegated_routing/
│   │   ├── mod.rs
│   │   ├── http_api.rs      — HTTP discovery API
│   │   └── resolve_proxy.rs — proxied RESOLVE for lightweight clients
│   ├── replication/
│   │   ├── mod.rs
│   │   ├── manager.rs       — replication peer selection and scheduling
│   │   └── health.rs        — replication health monitoring
│   └── telemetry/
│       ├── mod.rs
│       ├── billing_metrics.rs — per-tenant revenue metrics
│       └── alerts.rs         — webhook alerting
├── portal-frontend/         — static frontend (Astro or htmx)
│   ├── src/
│   │   ├── pages/
│   │   │   ├── index.astro          — landing page
│   │   │   ├── signup.astro         — Stripe Checkout flow
│   │   │   ├── login.astro
│   │   │   └── dashboard/
│   │   │       ├── index.astro      — overview
│   │   │       ├── identities.astro
│   │   │       ├── descriptors.astro
│   │   │       ├── usage.astro
│   │   │       └── billing.astro
│   │   └── components/
│   └── public/
├── deploy/
│   ├── Dockerfile
│   ├── docker-compose.yml   — hub + portal + monitoring stack
│   ├── fly.toml             — Fly.io deployment (or similar)
│   └── terraform/           — infrastructure as code (future)
├── config/
│   ├── purposebot-hub.toml    — production config template
│   └── purposebot-hub.dev.toml
├── Cargo.toml
└── README.md
```

### 1.2 Dependency Model

```toml
# purposebot-hub/Cargo.toml
[dependencies]
mesh-core      = { git = "https://github.com/purposebot/mesh-protocol", branch = "main" }
mesh-dht       = { git = "https://github.com/purposebot/mesh-protocol", branch = "main" }
mesh-transport = { git = "https://github.com/purposebot/mesh-protocol", branch = "main" }
mesh-hub       = { git = "https://github.com/purposebot/mesh-protocol", branch = "main" }
mesh-schemas   = { git = "https://github.com/purposebot/mesh-protocol", branch = "main" }

# PurposeBot-specific dependencies
stripe-rust    = "..."        # Stripe API client
axum           = "..."        # HTTP framework (admin API + portal)
askama         = "..."        # HTML templates
sqlx           = { version = "...", features = ["sqlite"] }  # tenant DB
moka           = "..."        # in-memory cache (if not already in mesh-hub)
governor       = "..."        # rate limiting (if not already in mesh-hub)
```

When mesh-protocol publishes to crates.io, switch from git dependencies
to versioned crate dependencies.

### 1.3 Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    purposebot-hub binary                       │
├────────────────┬────────────────┬───────────────────────────┤
│  Web Portal    │  Payment Svc   │  Premium Services         │
│  (Astro +      │  (Stripe +     │  (Delegated Routing +     │
│   Axum routes) │   MU Metering) │   Replication)            │
├────────────────┴────────────────┴───────────────────────────┤
│                                                             │
│           mesh-hub (open source, used as library)           │
│                                                             │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌───────────────┐  │
│  │ Hub      │ │ Storage  │ │ Policy   │ │ Admin API     │  │
│  │ Peering  │ │ Manager  │ │ Engine   │ │ (base routes) │  │
│  └──────────┘ └──────────┘ └──────────┘ └───────────────┘  │
├─────────────────────────────────────────────────────────────┤
│              mesh-core / mesh-dht / mesh-transport           │
└─────────────────────────────────────────────────────────────┘
```

The PurposeBot binary wraps mesh-hub, extending it with:
- Additional Axum routes (portal, payment webhooks, delegated routing)
- Payment middleware (intercepts tenant operations, applies MU metering)
- Replication background service
- Custom telemetry (billing metrics, revenue tracking)

---

## 2. What mesh-hub Must Expose

For purposebot-hub to extend mesh-hub without forking, the open source
mesh-hub crate must expose clean extension points:

### 2.1 Required Public API from mesh-hub

```rust
// mesh-hub must expose these for private extensions:

/// The core hub runtime — start, configure, shut down
pub struct HubRuntime { ... }
impl HubRuntime {
    pub fn new(config: HubConfig) -> Self;
    pub async fn start(&self) -> Result<()>;
    pub async fn shutdown(&self) -> Result<()>;

    /// Access to internal subsystems
    pub fn storage(&self) -> &StorageManager;
    pub fn dht(&self) -> &DhtNode;
    pub fn transport(&self) -> &TransportManager;
    pub fn tenant_manager(&self) -> &TenantManager;
    pub fn admin_router(&self) -> axum::Router;  // base admin API routes
}

/// Storage manager — query and manage descriptors
pub trait StorageManager {
    async fn store(&self, descriptor: Descriptor) -> Result<StoreResult>;
    async fn find_by_routing_key(&self, key: &Hash) -> Vec<Descriptor>;
    async fn find_by_publisher(&self, id: &Identity) -> Vec<Descriptor>;
    async fn find_by_tenant(&self, tenant_id: Uuid) -> Vec<Descriptor>;
    fn stats(&self) -> StorageStats;
}

/// Tenant manager — CRUD on tenants and identities
pub trait TenantManager {
    async fn create_tenant(&self, req: CreateTenantReq) -> Result<Tenant>;
    async fn get_tenant(&self, id: Uuid) -> Option<Tenant>;
    async fn get_tenant_by_identity(&self, id: &Identity) -> Option<Tenant>;
    async fn register_identity(&self, tenant_id: Uuid, id: Identity) -> Result<()>;
    async fn update_tenant(&self, id: Uuid, update: TenantUpdate) -> Result<Tenant>;
    async fn list_tenants(&self) -> Vec<Tenant>;
}

/// Hook point for intercepting protocol operations
pub trait ProtocolHook: Send + Sync {
    /// Called before a STORE is accepted. Return Err to reject.
    async fn pre_store(&self, descriptor: &Descriptor, tenant: Option<&Tenant>) -> Result<()>;

    /// Called after a STORE is accepted.
    async fn post_store(&self, descriptor: &Descriptor, tenant: Option<&Tenant>);

    /// Called before a FIND_VALUE result is returned.
    async fn pre_query(&self, key: &Hash, requester: &Identity) -> Result<()>;

    /// Called after a FIND_VALUE result is returned.
    async fn post_query(&self, key: &Hash, requester: &Identity, result_count: usize);
}
```

### 2.2 Extension Model

purposebot-hub registers hooks with the hub runtime:

```rust
// purposebot-hub/src/main.rs (conceptual)
#[tokio::main]
async fn main() {
    let config = load_config();

    // Start the open source hub runtime
    let hub = HubRuntime::new(config.hub);

    // Register PurposeBot-specific hooks
    let metering = MeteringHook::new(config.stripe, hub.tenant_manager());
    hub.register_hook(Arc::new(metering));

    // Extend the admin API with PurposeBot routes
    let app = hub.admin_router()
        .merge(portal_routes(config.portal))
        .merge(payment_routes(config.stripe))
        .merge(delegated_routing_routes(hub.storage(), hub.dht()));

    // Start replication service
    let replication = ReplicationManager::new(hub.storage(), hub.dht(), config.replication);
    tokio::spawn(replication.run());

    // Start everything
    hub.start().await.unwrap();
}
```

This means:
- mesh-hub handles all protocol operations, DHT, storage, peering
- purposebot-hub intercepts operations via hooks (metering, billing)
- purposebot-hub adds HTTP routes (portal, payment, delegated routing)
- purposebot-hub adds background services (replication, billing sync)
- No forking. No patching. Clean composition.

---

## 3. Infrastructure

### 3.1 Initial Deployment (Phase 0)

Minimum viable infrastructure for the first PurposeBot hub:

```
┌──────────────────────────────────────────┐
│              Fly.io (or similar)          │
│                                          │
│  ┌──────────────────────────────────┐    │
│  │  purposebot-hub                    │    │
│  │  - QUIC :4433 (mesh protocol)   │    │
│  │  - HTTP :8080 (admin API)       │    │
│  │  - HTTP :3000 (web portal)      │    │
│  │  - redb: /data/descriptors      │    │
│  │  - SQLite: /data/tenants.sqlite │    │
│  └──────────────────────────────────┘    │
│                                          │
│  Volume: /data (persistent SSD)          │
└──────────────────────────────────────────┘

DNS:
  hub.purposebot.ai        → Fly.io (QUIC + admin API)
  portal.purposebot.ai     → Fly.io (web portal)
```

Single instance. Single region. Persistent volume for storage. This
serves the first 100 tenants and validates the model.

Cost estimate: ~$30-50/month (2 vCPU, 4GB RAM, 40GB SSD on Fly.io).

### 3.2 Growth Deployment (Phase 1+)

When single-instance isn't enough:

```
┌──────────────────────┐  ┌──────────────────────┐
│  US-East Hub         │  │  EU-West Hub         │
│  purposebot-hub        │◄─►  purposebot-hub        │
│  (hub peering)       │  │  (hub peering)       │
└──────────┬───────────┘  └──────────┬───────────┘
           │                         │
     ┌─────▼─────┐            ┌──────▼────┐
     │  Shared   │            │  Shared   │
     │  Stripe   │            │  Stripe   │
     │  Account  │            │  Account  │
     └───────────┘            └───────────┘
```

Each region runs an independent hub instance with its own storage.
Hub peering (from PLAN-01 Section 3) handles cross-region descriptor
replication. Stripe account is shared (single billing entity).

Tenants are homed to a region but their descriptors replicate to peers.

---

## 4. Build Phases

### Phase 0: Foundation (status: planned)
**Goal:** PurposeBot hub binary running with Stripe payments.
**Depends on:** mesh-hub Phase 0 complete (core hub working).

- [ ] Create `purposebot-hub` private repo
- [ ] Cargo workspace with mesh-protocol git dependencies
- [ ] Config loader (extends mesh-hub config with Stripe, portal settings)
- [ ] Main binary that wraps HubRuntime
- [ ] Stripe integration:
  - [ ] Stripe Checkout session creation
  - [ ] Webhook handler (payment succeeded/failed, subscription changes)
  - [ ] Usage Records reporting (60s interval)
- [ ] MU metering:
  - [ ] In-memory MU balance tracking (AtomicI64)
  - [ ] ProtocolHook implementation for MU deduction
  - [ ] Balance flush to SQLite (5s interval)
  - [ ] Quota enforcement (soft/hard limits)
- [ ] Trial account flow:
  - [ ] POST /api/v1/account/register (agent-initiated)
  - [ ] 10K MU trial budget, 7-day expiry
  - [ ] DID-auth challenge-response
- [ ] Deploy to Fly.io (single instance)
- [ ] DNS setup (hub.purposebot.ai)

**Milestone:** A provider can discover the hub on the mesh, register an
account, publish a capability descriptor, and pay for the service.

### Phase 1: Web Portal (status: planned)
**Goal:** Human-friendly onboarding and management.
**Depends on:** Phase 0 complete.

- [ ] Portal frontend (Astro or htmx):
  - [ ] Landing page with pricing
  - [ ] Signup flow → Stripe Checkout
  - [ ] Login (email/password or OAuth)
  - [ ] Dashboard: account overview, MU usage gauge
  - [ ] Identity management (register/remove DIDs)
  - [ ] Descriptor viewer
  - [ ] Usage history and charts
  - [ ] Billing: current plan, invoices, upgrade
- [ ] Portal backend (Axum routes, session auth)
- [ ] Deploy portal alongside hub

**Milestone:** A human can sign up, pay, register their agents, and
monitor usage through a web browser.

### Phase 2: Delegated Routing (status: planned)
**Goal:** HTTP-based capability discovery for lightweight clients.
**Depends on:** Phase 0 complete.

- [ ] HTTP discovery API:
  - [ ] GET /api/v1/discover?type=...&limit=...
  - [ ] Filter support (available_now, max_price, geo)
  - [ ] MU metering (1 MU per query)
- [ ] HTTP resolve proxy:
  - [ ] GET /api/v1/resolve/{descriptor_id}
  - [ ] Hub makes outbound QUIC RESOLVE on behalf of client
  - [ ] MU metering (5 MU per resolve)
- [ ] Client SDK (Python):
  - [ ] `pip install purposebot-mesh`
  - [ ] `client = MeshClient("hub.purposebot.ai", did=my_did)`
  - [ ] `results = client.discover("compute/inference/text-generation")`
  - [ ] `status = client.resolve(descriptor_id)`
- [ ] Client SDK (TypeScript):
  - [ ] `npm install @purposebot/mesh-client`
  - [ ] Same interface as Python SDK

**Milestone:** An agent using Python or TypeScript can discover and
resolve capabilities with 3 lines of code, no QUIC, no DHT.

### Phase 3: Replication (status: planned)
**Goal:** High-availability descriptor storage across hubs.
**Depends on:** Phase 0 complete, multiple hub instances running.

- [ ] Replication manager:
  - [ ] Peer hub selection (geographic diversity)
  - [ ] STORE-based replication to peer hubs
  - [ ] Republish tracking (ensure replicas stay fresh)
- [ ] Replication health monitoring
- [ ] Per-tenant replication config (in dashboard)
- [ ] Second hub instance (EU-West) on Fly.io
- [ ] Cross-region replication testing

**Milestone:** A tenant's descriptors survive the loss of their home hub.

### Phase 4: Growth (status: planned)
**Goal:** Scale, polish, ecosystem.
**Depends on:** Phases 0-3 complete.

- [ ] Agent framework integrations:
  - [ ] LangChain tool for mesh discovery
  - [ ] CrewAI integration
  - [ ] OpenAI function calling adapter
- [ ] API key auth (alternative to DID-auth for HTTP API)
- [ ] Operator billing dashboard (revenue, per-tenant breakdown)
- [ ] Public status page (hub health, network stats)
- [ ] Getting Started guide (5 minutes to first capability)
- [ ] Blog post / Hacker News launch
- [ ] Mesh voucher system (agent-native payment)

---

## 5. Upstream Requirements

Things that must exist in the open source mesh-hub before purposebot-hub
can build on them. These should be tracked as issues/PRs on mesh-protocol.

| Requirement | mesh-hub component | Needed by |
|-------------|-------------------|-----------|
| HubRuntime public API | mesh-hub lib | Phase 0 |
| ProtocolHook trait | mesh-hub lib | Phase 0 |
| TenantManager public API | mesh-hub lib | Phase 0 |
| StorageManager public API | mesh-hub lib | Phase 0 |
| Admin API as composable Axum Router | mesh-hub lib | Phase 0 |
| Hub peering (for replication targets) | mesh-hub | Phase 3 |

The key design constraint: mesh-hub must be usable as a **library** (not
just a binary). purposebot-hub composes mesh-hub's components into its own
binary. This means mesh-hub needs a clean `lib.rs` that exposes the
runtime, managers, and extension points listed in Section 2.1.

If mesh-hub only ships as a binary with no library interface, purposebot-hub
would be forced to fork. The library interface is what prevents forking.

---

## 6. Revenue Model

| Revenue Stream | Available | Pricing |
|---------------|-----------|---------|
| Hub subscriptions (Pro/Enterprise) | Phase 0 | $49-499/month |
| MU overage billing | Phase 0 | $0.005-0.01 per 1K MU |
| Delegated routing API | Phase 2 | Included in MU budget |
| Descriptor replication | Phase 3 | Included in Pro/Enterprise tiers |
| Agent framework SDKs | Phase 4 | Free (drives hub usage) |

**Break-even estimate:** ~50 Pro tenants or ~5 Enterprise tenants covers
infrastructure costs ($30-50/month) plus Stripe fees. Revenue beyond that
is margin.

---

## 7. Competitive Moat

1. **First mover** — first hub on the mesh. All early adopters are your tenants.
2. **Network effects** — the more providers on your hub, the more valuable
   discovery is for consumers, and vice versa.
3. **Delegated routing** — the HTTP API makes your hub the easiest on-ramp.
   Agents that use your SDK are locked into your hub by convenience, not contract.
4. **Replication partnerships** — bilateral replication agreements with other
   hub operators create a web of interdependency.
5. **Open source credibility** — you maintain the reference implementation.
   The community trusts you because they can read the code.
