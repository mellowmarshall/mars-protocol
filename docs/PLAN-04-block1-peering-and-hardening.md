# PLAN-04: Block 1 — Hub Peering + DHT Security Hardening

**Status:** in-progress
**Depends on:** Block 0 (wire protocol stabilization) ✅
**Parallel streams:** 2 (no file overlap)

---

## Stream A: Phase 1 Hub Peering — status: planned

### A1: Infrastructure routing key constant — mesh-schemas
- Add `ROUTING_KEY_INFRASTRUCTURE_HUB: LazyLock<Hash>` = `routing_key("infrastructure/hub")`
- Add `ROUTING_KEY_INFRASTRUCTURE: LazyLock<Hash>` = `routing_key("infrastructure")`

### A2: Hub self-advertisement — mesh-hub
- On startup, HubRuntime publishes a descriptor:
  - schema: `SCHEMA_HASH_INFRA_HUB`
  - topic: `"hub"`
  - routing_keys: `[ROUTING_KEY_INFRASTRUCTURE, ROUTING_KEY_INFRASTRUCTURE_HUB]`
  - payload: CBOR-encoded hub metadata (capacity, endpoint, regions)
- Re-publish periodically (TTL/2 interval) to keep descriptor alive

### A3: Hub discovery — mesh-hub
- On startup (after self-advertisement), FIND_VALUE at `ROUTING_KEY_INFRASTRUCTURE_HUB`
- Parse discovered hub descriptors to extract peer endpoints
- Filter self (skip own DID)
- Attempt QUIC connections to discovered peers

### A4: Peer connection pool — mesh-hub
- `PeerManager` struct: maintains persistent QUIC connections to peer hubs
- Connect to discovered peers, store `MeshConnection` handles
- Reconnect on disconnect (exponential backoff)
- Track peer state: Connected, Disconnected, Unhealthy

### A5: Gossip protocol — mesh-hub
- Periodic push (every 60s): send recent descriptor updates to connected peers via STORE
- Batch limit: max 50 descriptors per gossip round
- Use existing STORE/FIND_VALUE messages (no new wire protocol)

### A6: Health monitoring — mesh-hub
- PING/PONG keep-alive every 10s to each connected peer
- Track consecutive failures → mark Unhealthy → disconnect after threshold
- Hub flags: Stable, HighCapacity, FullTable, Verified (informational, stored in peer metadata)

### A7: Wire into HubRuntime — mesh-hub
- Add PeerManager to HubRuntime
- Spawn peering background task in `run()`
- Graceful shutdown: drain peer connections during shutdown period

---

## Stream B: DHT Security Hardening — status: planned

### B1: LRU ping challenge (Security #1) — mesh-dht/src/routing.rs
- Current: `add_node` evicts LRS entry immediately when bucket full
- Fix: Return `AddNodeResult` enum: `Added`, `Updated`, `BucketFull { lrs: NodeInfo, candidate: NodeInfo }`
- Caller (DhtNode) handles BucketFull: PING the LRS node
  - If LRS responds: keep LRS, discard candidate, move LRS to tail
  - If LRS doesn't respond: evict LRS, add candidate
- Keep routing table as pure data structure (no I/O)

### B2: Revocation enforcement (Security #2) — mesh-dht/src/storage.rs
- Parse `core/revocation` descriptors on store:
  - Extract `target_id` from payload
  - Remove target descriptor from store
  - Add target_id to revocation index (HashSet<Hash>)
- On `get_descriptors`: filter out revoked descriptor IDs
- Validate: revocation descriptor MUST be signed by same publisher as target
- Store revocation descriptors themselves (so they propagate through the network)

### B3: Key-rotation handling (Security #2) — mesh-dht/src/storage.rs
- Parse `core/key-rotation` descriptors on store:
  - Extract old_identity, new_identity, rotation_seq
  - Verify dual signatures (old key signs new, new key signs old)
  - Check rotation_seq > previous for same old_identity
  - Maintain identity_map: old_identity → (new_identity, rotation_seq)
- On `get_descriptors`: descriptors from old_identity are still returned
  (rotation doesn't invalidate existing descriptors, only establishes successor)

### B4: Replay watermark persistence (Security #3) — mesh-hub/src/storage/redb.rs
- Add `SEQUENCES` table (already exists): `(publisher||schema_hash||topic) → u64`
- On `store_descriptor`: persist sequence floor after accepting descriptor
- On startup: load all sequence floors from SEQUENCES table into memory
- In-memory DescriptorStore (mesh-dht): no change (ephemeral nodes don't need persistence)

---

## Agent Assignment

| Agent | Crate(s) | Tasks | New Files |
|-------|----------|-------|-----------|
| **Hub Peering** | mesh-hub, mesh-schemas | A1–A7 | mesh-hub/src/peering.rs |
| **DHT Hardening** | mesh-dht, mesh-hub/src/storage | B1–B4 | — |

No file overlap between streams.
