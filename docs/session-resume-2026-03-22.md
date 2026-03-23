## Resume Context — Mesh Agent Routing Standard (MARS) Design Session

**Date:** 2026-03-22
**Project:** `/home/logan/Dev/mesh-protocol`

---

### What this project is

The Mesh Agent Routing Standard (MARS) — a Kademlia-based DHT protocol for
decentralized capability discovery. Agents publish signed capability
descriptors and other agents discover them via content-addressed routing
keys over QUIC. Think DNS for machine capabilities, with no central authority.

Two repos:
- `mesh-protocol` (public, open source) — the protocol spec, reference node, reference hub
- Private hub implementation hosted at **purposebot.ai** (existing site for MCP, API, and WebMCP)

The private repo imports mesh-protocol crates as dependencies. NOT a fork.

---

### What's been built (source code at /home/logan/Dev/mesh-protocol)

4 crates implemented and working:
- **mesh-core** — Descriptor (with `topic` field), Hash, Identity, Keypair, Frame, all 8 message types, canonical CBOR serialization (BTreeMap), BLAKE3 hashing, Ed25519 signing, well-known schema hashes, routing key derivation
- **mesh-dht** — DhtNode, RoutingTable (256 k-buckets, K=20), DescriptorStore (in-memory HashMap), iterative lookups, all message handlers, per-publisher rate limiting
- **mesh-transport** — MeshEndpoint (QUIC via quinn), MeshConnection, frame send/recv, self-signed TLS cert generation
- **mesh-node** — CLI binary with start, publish, discover, ping, identity commands

Test coverage across all crates including integration tests.

**Known issue being fixed:** ciborium dependency being replaced — serialization rewritten with BTreeMap<String, Value> for deterministic canonical CBOR across all language implementations.

---

### Design docs (move to /home/logan/Dev/mesh-protocol going forward)

All planning docs have been migrated from `/home/logan/.openclaw/workspace/mesh`
into the source repo under `docs/`. That workspace is retired as a planning
space — everything is now self-contained in mesh-protocol.

**PROTOCOL.md** — Complete wire specification. All 6 open questions resolved as design decisions (Section 13). Section 14 added for protocol evolution. Key spec changes:
- `topic` field added to Descriptor (dedup key = publisher + schema_hash + topic)
- Timestamp future guard (120s tolerance)
- All deferred items explicitly state why they're not needed yet and what triggers them

**PLAN-01-mesh-hub.md** — Open source hub design:
- 3-tier storage (L1 in-memory cache, L2 redb, L3 SQLite for tenants)
- Hub peering with BGP-style full hub list, gossip batching, hub flags
- Multi-tenant: accounts with multiple DIDs, tiered quotas (free/pro/enterprise)
- Mesh Units billing abstraction (payment-agnostic in open source)
- Security: connection rate limits, admin API lockdown, blocklists, policy engine
- Operational lifecycle: graceful shutdown, backup/recovery, migration from mesh-node
- **Pre-Phase: Upstream Refactors** — MUST land before mesh-hub can be built:
  1. Extract `DescriptorStorage` trait from concrete `DescriptorStore` in mesh-dht
  2. Add `ProtocolHook` trait (pre_store, post_store, pre_query, post_query)
  3. Make `DhtNode` generic over storage
  4. Expose public API for downstream composition
- **Library interface** (Section 1.3) — mesh-hub must expose HubRuntime builder, TenantManager, StorageManager, admin Router as composable Axum Router

**docs/private/PLAN-02-purposebot-hub.md** — purposebot.ai hub implementation:
- Separate repo, imports mesh-protocol crates as git dependencies
- Stripe integration, MU metering with atomic in-memory balance, async flush
- Trial accounts for agent-initiated registration
- Web portal (Astro/htmx) at purposebot.ai
- Delegated routing (HTTP discovery API — the Infura model)
- Descriptor replication across peer hubs
- 4 build phases: MVP → Portal → Delegated Routing → Replication

**docs/private/hub-payment-implementation.md** — Payment/entitlement design:
- Zero-I/O hot path: connection cache → DashMap tenant cache → token bucket → atomic MU decrement
- Stripe Checkout + Usage Records + webhooks
- Grace periods and suspension policies

**docs/review-prompts.md** — 4 review prompts ready to hand to agents:
1. Senior SWE (correctness, scalability, serialization, deps)
2. Security & Hardening (crypto attacks, protocol attacks, hub attack surface, privacy)
3. Interoperability & Spec Ambiguity (CBOR determinism, hash input ambiguity, test vectors)
4. Cryptography (Ed25519 usage, signature scheme, post-quantum readiness)

---

### Key design decisions (for quick reference)

- Capability types: fully emergent, no governance, market curation
- Incentives: protocol-agnostic, providers self-incentivize, hubs are ISPs
- Cross-mesh: one bridge node merges networks, hubs maintain full hub peer lists
- Encryption: none at protocol layer, auth at endpoint, private meshes for sensitive
- Upgrades: dual-stack, no coordination, version byte in frame header
- Compliance: protocol is neutral infrastructure, operator's responsibility
- Evolution: 3 layers (wire protocol / schemas / behavioral conventions)
- Hub tenancy: account-based (not identity-based), multiple DIDs per account
- Payment: Mesh Units normalize all operations, billing is external to open source

---

### Next steps (in priority order)

1. **Review the SWE review results** — first review is complete, needs 2nd opinion before implementing changes
2. ~~**Migrate planning docs**~~ — DONE, docs now in `docs/` and `docs/private/`
3. **Pre-Phase upstream refactors** — extract DescriptorStorage trait, add ProtocolHook trait, make DhtNode generic
4. **Build mesh-hub** — Phase 0 per PLAN-01 (disk storage, tenant manager, admin API, security, library interface)
5. **Create purposebot.ai hub repo** — Phase 0 per PLAN-02 (Stripe, MU metering, deploy)
6. **Stand up seed nodes** — 2-3 public VPSes running mesh-node
7. **Ship it** — getting started guide, open source, announce
