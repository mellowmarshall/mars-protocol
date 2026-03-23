# Senior Engineer Review - mesh-protocol

Source rubric: `/home/logan/Dev/Senior-Engineer-Review-Full.md`

Evaluated against this Rust workspace on 2026-03-22. No web/mobile app exists here, so Gate 1 was applied to the CLI and DHT capabilities. `cargo test` passed locally.

## A) Final Verdict

`REQUEST_CHANGES`

## B) Gate Results

- Gate 1 (Completeness/Integration): `FAIL`
- Gate 2 (Rigor): `FAIL`
- Merge eligibility: `BLOCKED`

## C) Blocking Issues

- Gate 1: `Discover` is only a direct `FIND_VALUE` to the seed and never uses the existing iterative lookup path, so discovery fails when the seed only knows closer nodes instead of storing the value itself. Evidence: `mesh-node/src/main.rs:375`, `mesh-node/src/main.rs:385`, `mesh-dht/src/node.rs:171`.
- Gate 1: bootstrap cannot reliably join from a lone seed. `bootstrap()` only records nodes returned by `FIND_NODE`, `handle_find_node()` does not include the seed itself, and `touch_sender()` is a no-op. A seed with an empty routing table teaches nothing. Evidence: `mesh-node/src/main.rs:237`, `mesh-dht/src/node.rs:267`, `mesh-dht/src/node.rs:305`, `mesh-dht/src/node.rs:623`.
- Gate 1: the implementation does not satisfy the protocol's own "every message is signed" rule. Control messages are plain CBOR structs and the QUIC client accepts any certificate. Evidence: `PROTOCOL.md:18`, `PROTOCOL.md:95`, `mesh-core/src/message.rs:46`, `mesh-transport/src/tls.rs:18`, `mesh-transport/src/tls.rs:101`.
- Gate 2: CLI pings advertise `0.0.0.0:0`, so peers learn unusable addresses and poison routing state. Evidence: `mesh-node/src/main.rs:361`, `mesh-node/src/main.rs:423`, `mesh-dht/src/node.rs:95`.

## D) Integration Matrix

| Capability | Step | Status | Evidence | Gap | User Impact |
|---|---|---|---|---|---|
| Start/bootstrap | CLI entry and listener wiring | Yes | `mesh-node/src/main.rs:213`, `mesh-node/src/main.rs:251` | None | Node can start and accept QUIC requests |
| Start/bootstrap | Join via single seed | No | `mesh-dht/src/node.rs:267`, `mesh-dht/src/node.rs:305` | Seed is never learned unless it already returns peers | Fresh nodes can remain disconnected |
| Publish | CLI builds descriptor and sends `STORE` | Yes | `mesh-node/src/main.rs:282`, `mesh-node/src/main.rs:320`, `mesh-dht/src/storage.rs:110` | None for single-hop store | Capability can be written to one reachable seed |
| Publish | Mesh-wide availability | Partial | `mesh-node/src/main.rs:309` | Only one seed is written; no iterative replication/store path | Discovery depends on that seed holding the value |
| Discover | CLI entry and response rendering | Yes | `mesh-node/src/main.rs:341`, `mesh-node/src/main.rs:389` | None | Command runs and prints results |
| Discover | Multi-hop DHT lookup | No | `mesh-node/src/main.rs:375`, `mesh-dht/src/node.rs:171` | Bypasses `lookup_value()` entirely | Misses capabilities unless seed already stores them |
| Ping | QUIC ping/pong roundtrip | Yes | `mesh-node/src/main.rs:415`, `mesh-node/tests/integration.rs:219` | None | Liveness check works |
| Auth | Signed/verified messages on runtime path | No | `PROTOCOL.md:18`, `mesh-core/src/message.rs:46`, `mesh-transport/src/tls.rs:25` | No authenticated control plane | Sender identity is forgeable |

## E) Rigor Findings

1. `High` | Unnecessary complexity | `cmd_discover()` reimplements a weaker single-hop lookup and even sends a `PING` whose result is ignored instead of reusing `DhtNode::lookup_value()`. Evidence: `mesh-node/src/main.rs:353`, `mesh-node/src/main.rs:375`, `mesh-dht/src/node.rs:171`. Why it matters: two discovery paths now exist, and the user-facing one is the wrong one. Minimal fix: make the CLI seed a local `DhtNode` and call `lookup_value()`.
2. `High` | Technical debt | routing-table maintenance is half-wired: `touch_sender()` is a stub, and CLI pings publish unroutable addresses. Evidence: `mesh-dht/src/node.rs:112`, `mesh-dht/src/node.rs:305`, `mesh-node/src/main.rs:361`. Why it matters: peer knowledge decays or becomes wrong, which directly hurts bootstrap and lookup quality. Minimal fix: carry valid sender addresses on all peer-discovery paths and implement actual touch/update behavior.
3. `Medium` | Performance regression | every request opens a fresh QUIC connection, including lookup/bootstrap loops. Evidence: `mesh-node/src/transport.rs:27`, `mesh-node/src/transport.rs:33`, `mesh-dht/src/node.rs:202`. Why it matters: iterative lookups pay repeated handshakes and cannot scale well. Minimal fix: cache/reuse `MeshConnection`s per peer.
4. `High` | Security risk | unsigned control messages plus trust-all TLS means any peer can spoof `sender` identities. Evidence: `PROTOCOL.md:18`, `mesh-core/src/message.rs:48`, `mesh-transport/src/tls.rs:33`, `mesh-transport/src/tls.rs:108`. Why it matters: routing, discovery, and peer metadata are unauthenticated. Minimal fix: sign and verify control messages before dispatch. Better alternative: bind transport identity to the DID and reject mismatches.

## F) Test Coverage Map

- Existing: `mesh-transport/src/tests.rs:34` covers QUIC frame exchange; `mesh-node/tests/integration.rs:108` proves direct remote lookup against a node that already stores the descriptor; `mesh-dht/src/node.rs:688` proves multi-hop lookup only under mock transport; `mesh-dht/src/node.rs:623` proves bootstrap only when the seed already has peers.
- Missing before merge: black-box CLI test for `discover` across at least 3 nodes; bootstrap test against a single empty seed; control-message signature/auth test; test that CLI pings advertise a usable local address.

## G) Merge Checklist

1. Make the runtime match the spec on message authentication, or change the spec to match the current unsigned design.
2. Replace `cmd_discover()`'s direct seed query with `DhtNode::lookup_value()`.
3. Fix bootstrap so a node can learn a lone seed, and implement real sender-touch/routing updates.
4. Stop advertising `0.0.0.0:0`; send the actual local endpoint address.
5. Add black-box integration tests for the actual CLI subcommands, not just the lower-level handlers.
