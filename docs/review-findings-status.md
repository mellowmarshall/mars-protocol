# Review Findings Status

**Last updated:** 2026-03-23

Consolidated findings from 4 code reviews:
1. Senior SWE Review (`docs/senior-engineer-review-mesh-protocol-2026-03-22.md`)
2. Cryptography Review (`docs/reviews/cryptography-review-2026-03-23.md`)
3. Interoperability & Spec Ambiguity Review (`docs/reviews/interoperability-spec-ambiguity-review-2026-03-23.md`)
4. Security & Hardening Review (`docs/reviews/security-hardening-review-2026-03-23.md`)

---

## Implemented Fixes

### SWE Review (all 6 findings — complete)

| # | Finding | Fix | Files Changed |
|---|---------|-----|---------------|
| 1 | `cmd_discover` bypasses iterative lookup | Rewrote to bootstrap `DhtNode` + `lookup_value()` | `mesh-node/src/main.rs` |
| 2 | Bootstrap can't learn a lone seed | `handle_find_node()` includes self in results | `mesh-dht/src/node.rs` |
| 3 | Messages unsigned / TLS trusts all certs | TLS identity binding: certs derived from Ed25519 mesh keypair; `peer_mesh_identity()` extracts identity from TLS cert | `mesh-transport/src/tls.rs`, `connection.rs`, `endpoint.rs` |
| 4 | CLI pings advertise `0.0.0.0:0` | All commands use real local address from `transport.local_addr()` | `mesh-node/src/main.rs` |
| 5 | Fresh QUIC connection per request | `QuicTransport` caches connections in `HashMap<SocketAddr, MeshConnection>` | `mesh-node/src/transport.rs` |
| 6 | `touch_sender()` is a no-op stub | Replaced with `update_routing_table()` using new `sender_addr` field on all request messages | `mesh-dht/src/node.rs`, `mesh-core/src/message.rs` |

### Cross-Review Pre-Refactor Fixes (all 6 — complete)

| # | Finding | Sources | Fix | Files Changed |
|---|---------|---------|-----|---------------|
| 1 | DID missing multibase `z` prefix | Crypto #3, Interop #7, Security #9 | `did()` now emits `did:mesh:z<base58btc(...)>` | `mesh-core/src/identity.rs` |
| 2 | Routing key prefix mismatch in spec | Crypto #7, Interop #2 | Fixed errant `mesh:routing:` → `mesh:route:` on PROTOCOL.md line 1246 (code was already correct) | `PROTOCOL.md` |
| 3 | Secret key file permissions / exposure | Crypto #6, Security #12 | Key files written with `0600`; removed raw hex printing from `identity --generate` | `mesh-node/src/main.rs` |
| 4 | `max_results` not clamped server-side | Security #8 | `handle_find_value` clamps to `config.max_find_value_results` | `mesh-dht/src/node.rs` |
| 5 | Sender identity not validated against TLS | Security #1-2, Crypto #1 | `listen()` passes `Option<Identity>` from TLS cert; `verify_sender()` rejects mismatches | `mesh-transport/src/endpoint.rs`, `mesh-node/src/main.rs` |
| 6 | CLI publishes JSON payload instead of CBOR | Interop #9 | `cmd_publish` serializes payload as deterministic CBOR with BTreeMap | `mesh-node/src/main.rs`, `mesh-node/Cargo.toml` |

### Previously Implemented (prior sessions)

- Canonical CBOR serialization with BTreeMap for deterministic content-hashing
- `sender_addr: NodeAddr` added to `Store`, `FindNode`, `FindValue` message types

---

## Spec Fixes (all 12 — complete)

| # | Finding | Sources | Fix |
|---|---------|---------|-----|
| 1 | Commit normatively to TLS-binding auth model | Crypto #2, Interop #5, Security #1 | Added Section 3.1.1, Section 8.2; deferred `Signed<T>` in Section 1.4; added TLS note to Section 9.4 |
| 2 | Add `sender_addr` to spec Sections 3.5-3.7 | Interop #4 | Added `sender_addr: NodeAddr` to STORE, FIND_NODE, FIND_VALUE wire formats |
| 3 | Mandate deterministic CBOR wire format | Interop #3, #14 | Added Section 1.5 with 5 normative rules (one item, no tag 55799, no dup keys, no indef-length, no trailing) |
| 4 | Add normative test vectors | Interop #10 | Added Appendix C.4 with DID derivation, node ID, schema hash, routing key vectors; fixed routing key prefix in descriptor test vector |
| 5 | Fix float fields in schemas | Interop #6 | `center: [int, int]` (microdegrees), `radius_m: uint`, `current_load_permille: uint`, `min_capacity_permille: uint` |
| 6 | Key rotation state machine | Crypto #4 | Added `rotation_seq`, `new_signature`; defined monotonic counter, fork detection, staleness rejection |
| 7 | `FindValueResult` one-of semantics | Interop #13 | Normative rule: exactly one of `descriptors`/`nodes`; absent field omitted (not null); reject both/neither |
| 8 | Equal-sequence update semantics | Interop #11 | Equal seq + different ID = conflicting → reject; equal seq + same ID = idempotent → accept |
| 9 | CBOR tags 42-45 reserved note | Interop #12 | Appendix A: tags 42-45 reserved but NOT used in v0x01; MUST NOT emit, SHOULD ignore |
| 10 | Schema fields typed as `bstr` | Interop #8 | `descriptor_id`/`target_id`/`successor` → `Hash`; `requester`/`old_identity`/`new_identity` → `Identity` |
| 11 | Algorithm registry governance | Crypto #5, Security #14 | Content-hash = collision-free registration; user-defined 0x80-0xFF are local-only, no interop guarantee |
| 12 | PQ algorithm slots formalization | Crypto #8 | ML-DSA/ML-KEM reserved-pending-implementation; activation requires hybrid signing mode spec |

---

## Remaining: After Refactors / Hub Phase

These require significant implementation work and/or depend on infrastructure not yet built (persistent storage, hub runtime, etc.).

| # | Finding | Sources | Depends On |
|---|---------|---------|------------|
| 1 | Routing table poisoning — reachability verification, LRU ping challenge | Security #3 | Pre-phase refactors (ProtocolHook trait) |
| 2 | Revocation & key-rotation enforcement in descriptor store | Security #4 | DescriptorStorage trait, durable storage |
| 3 | Replay watermark persistence | Security #10 | Durable storage (mesh-hub Phase 0) |
| 4 | Resource exhaustion — per-IP quotas, proof-of-work, storage caps | Security #7 | mesh-hub security layer |
| 5 | Hub SSRF prevention | Security #5 | mesh-hub delegated routing (Phase 3) |
| 6 | Trial account abuse prevention | Security #6 | mesh-hub tenant system |
| 7 | DID-Auth challenge specification | Security #11 | mesh-hub auth system |
| 8 | Observability/tenant metadata leaks | Security #13 | mesh-hub observability layer |
| 9 | Voucher binding to holder DID | Security #15 | Voucher system (deferred) |

---

## Test Status

**146 tests passing, 0 failures** across all 6 crates after all fixes + refactors.

| Crate | Tests |
|-------|-------|
| mesh-core | 75 |
| mesh-dht | 53 |
| mesh-transport | 10 |
| mesh-schemas | 3 |
| mesh-client | 2 |
| mesh-node (integration) | 3 |
