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
