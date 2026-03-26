# Architecture Context

Decisions and rationale for major design choices. Read this before proposing
structural changes or flagging issues in code review.

---

## ADR-001: Hybrid Authentication Model

**Date:** 2026-03-23
**Status:** Accepted

**Decision:** Descriptors carry Ed25519 publisher signatures (self-authenticating).
Protocol messages (PING, STORE, FIND_NODE, FIND_VALUE) rely on mutual TLS
identity binding — no per-message signatures.

**Rationale:** At planetary M2M scale, signing every ephemeral message (billions/sec)
burns CPU for a guarantee nobody needs — the TLS handshake already proved identity.
Descriptors, however, must be self-authenticating because they traverse untrusted
intermediaries (hub federation, caching, relay). This matches the pattern used by
Bitcoin (signed txns, unsigned p2p), IPFS (content-addressed data, transport-secured
protocol), and DNSSEC (signed records, unsigned queries).

**Implications:**
- Hub federation is trustless — any node can verify any descriptor without trusting the hub
- Caching scales freely — signed descriptors can be served from any edge cache
- Future non-TLS transports can add `Signed<T>` per-message envelopes (§1.4.1)

---

## ADR-002: RFC 8949 §4.2.1 Deterministic CBOR for Content Hashing

**Date:** 2026-03-23
**Status:** Accepted

**Decision:** Descriptor content hash (ID) uses RFC 8949 §4.2.1 Core Deterministic
Encoding. Map keys are sorted by bytewise lexicographic order of their CBOR-encoded
form. For text string keys, shorter keys sort first (CBOR length prefix byte is
smaller), then lexicographic within same length.

**Canonical field order:** `ttl, topic, payload, sequence, publisher, timestamp, schema_hash, routing_keys`

**Rationale:** BTreeMap lexicographic string order (`payload, publisher, ...`) differs
from RFC 8949 encoded-key-bytes order (`ttl, topic, ...`). Any non-Rust implementation
using standard CBOR deterministic encoding would compute different descriptor IDs,
breaking interoperability. RFC 8949 is the IETF standard; aligning with it ensures any
language's CBOR library can produce correct hashes.

---

## ADR-003: Integer-Only Schemas (No CBOR Floats)

**Date:** 2026-03-23
**Status:** Accepted

**Decision:** All schema fields that could be floats use integer representations:
microdegrees (1e-6°) for coordinates, meters for distance, permille (0–1000) for
load/capacity, decimal strings for monetary amounts.

**Rationale:** CBOR permits multiple float encodings (half/single/double precision)
for the same value. Different implementations could produce different bytes, causing
different descriptor content hashes. Integer encoding is unambiguous.

---

## ADR-004: Iterative STORE as Primary Replication (Not Gossip)

**Date:** 2026-03-26
**Status:** Accepted

**Context:** Block 1 introduced hub-to-hub gossip for descriptor replication.
This created persistent QUIC connections between hubs with a push-based gossip
protocol. In practice, this led to cascading complexity: peer discovery
chicken-and-egg, rate limit bypass for peer traffic, one-directional connection
registration, and the network still didn't converge because gossip traffic was
being rate-limited by the very hooks designed to protect against abuse.

Standard Kademlia (BitTorrent BEP 5, libp2p Kademlia spec) solves replication
differently: the publisher sends STORE to the K closest nodes to the routing
key. Every message exchange updates routing tables automatically. No persistent
connections, no gossip protocol, no special peer tier.

**Decision:** Use standard Kademlia iterative STORE as the primary replication
mechanism. Keep hub gossip as a secondary consistency layer for hub metadata.

- `DhtNode::iterative_store()` finds K closest nodes to the key and sends
  STORE to all of them (mirrors `lookup_value` but writes instead of reads)
- Publisher re-publishes periodically (TTL-based) to keep data alive
- Every incoming message updates the sender in the routing table
- Gossip remains for `infrastructure/hub` advertisements only

**Rationale:**
- Proven at scale: BitTorrent DHT handles billions of lookups/day with this model
- Self-healing: new nodes discover data via normal FIND_VALUE lookups
- No persistent connections needed between hubs
- Publisher controls their own data lifecycle via re-publish interval
- Eliminates gossip rate-limit bypass, peer bootstrap, and connection tracking complexity
