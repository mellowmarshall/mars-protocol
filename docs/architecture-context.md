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

---

## ADR-005: Descriptor Handoff Schema (Auth + Protocol + Pricing)

**Date:** 2026-03-27
**Status:** Accepted

**Context:** Descriptors carry provider metadata in the `params` field, but the
format is inconsistent. Some entries use `"auth": "none"`, others use
`"auth": "HF_TOKEN"`, others have `"free_tier": "1,000 credits"`. An agent
parsing these has to guess the structure. We need a standard schema that any
agent SDK can parse programmatically.

MARS does NOT normalize provider APIs. Each provider speaks its own protocol
(OpenAI, Ollama, Vast, MCP, plain REST). The schema answers three questions:
1. How do I authenticate?
2. What protocol does the endpoint speak?
3. What does it cost?

After that, the agent talks directly to the provider. MARS hands off the
connection — it does not proxy, translate, or abstract the provider's API.

**Decision:** Standardize three fields in `params`:

### `auth` — how to authenticate

```json
// No auth needed
{"auth": {"method": "none"}}

// API key (agent or user provides)
{"auth": {"method": "api_key", "key_name": "GROQ_API_KEY", "header": "Authorization: Bearer {key}", "signup_url": "https://console.groq.com"}}

// Stripe payment (pay-per-use)
{"auth": {"method": "stripe", "checkout_url": "https://purposebot.ai/pay/{provider_id}"}}

// OAuth
{"auth": {"method": "oauth", "authorize_url": "https://provider.com/oauth/authorize", "scopes": ["inference"]}}
```

### `protocol` — what the endpoint speaks

One of: `openai`, `ollama`, `rest`, `mcp`, `vast`, `runpod`, `replicate`,
`huggingface`, or any string the agent can match against its available clients.

If `protocol` is `openai`, any OpenAI-compatible SDK works. If it's `ollama`,
the Ollama SDK works. If it's an unknown string, the agent falls back to raw
HTTP with the endpoint URL.

### `pricing` — what it costs

```json
// Free
{"pricing": {"model": "free"}}

// Per-token
{"pricing": {"model": "per-token", "price_per_mtok": 2.67, "currency": "USD"}}

// Per-hour (GPU rental)
{"pricing": {"model": "per-hour", "price_per_hour": 0.40, "currency": "USD"}}

// Freemium (free tier with limits)
{"pricing": {"model": "freemium", "free_tier": "1,000 requests/month", "paid_url": "https://provider.com/pricing"}}
```

**Full example:**

```json
{
  "type": "compute/inference/text-generation",
  "endpoint": "https://abc123.ngrok.io/v1/chat/completions",
  "params": {
    "name": "Llama 4 Scout (RTX 4090)",
    "model": "llama4:latest",
    "gpu": "NVIDIA GeForce RTX 4090",
    "vram_mb": 24576,
    "region": "us-east",
    "protocol": "openai",
    "auth": {"method": "none"},
    "pricing": {"model": "per-token", "price_per_mtok": 2.67, "currency": "USD"}
  }
}
```

**Rationale:**
- MARS is discovery, not abstraction. Like DNS resolves addresses but doesn't
  speak HTTP, MARS resolves capabilities but doesn't speak inference APIs.
- The `protocol` field tells the agent which SDK/client to use. This is the
  minimum information needed for a handoff.
- Pricing is descriptive, not enforced. The mesh doesn't process payments —
  that's between the agent and the provider (or via purposebot for brokered payments).
- Backward compatible: existing descriptors without these fields still work.
  Agents treat missing `auth` as unknown, missing `protocol` as `rest`,
  missing `pricing` as unknown.
