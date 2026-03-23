# Capability Mesh Protocol — Wire Specification

**Version:** 0.1.0-draft
**Status:** Design Draft
**Authors:** Marshall, Shikamaru
**Date:** 2026-03-22

---

## 0. Design Philosophy

This protocol is designed to outlast its creators.

**Core principles:**
1. The protocol has no opinions about what capabilities are.
2. The core never changes. Evolution happens in payload schemas.
3. Every identifier is a content-hash. No registries.
4. Every message is signed. No unsigned communication.
5. Algorithm agility is mandatory. No hardcoded cryptography.
6. No global state. Eventually consistent. Scales to billions.
7. The spec is the bytes on the wire. No canonical implementation.

---

## 1. Conventions

- All integers are unsigned, big-endian.
- All byte lengths are prefixed as `u32` unless stated otherwise.
- All hashes are expressed as `(algorithm_id: u8, digest: bytes)`.
- All strings are UTF-8, length-prefixed with `u16`.
- All timestamps are microseconds since Unix epoch, `u64`.
- The canonical serialization format is **CBOR** (RFC 8949) for all structured data.
- Wire framing uses a minimal binary envelope (Section 3).

### 1.1 Algorithm Registry

Algorithms are identified by a single byte. The initial registry:

| ID   | Algorithm       | Type      | Output Size | Status     |
|------|----------------|-----------|-------------|------------|
| 0x01 | Ed25519        | Signature | 64 bytes    | Required   |
| 0x02 | ML-DSA-65      | Signature | 3309 bytes  | Reserved   |
| 0x03 | BLAKE3         | Hash      | 32 bytes    | Required   |
| 0x04 | SHA-256        | Hash      | 32 bytes    | Supported  |
| 0x05 | X25519         | KeyExch   | 32 bytes    | Required   |
| 0x06 | ML-KEM-768     | KeyExch   | varies      | Reserved   |
| 0x80–0xFF | —         | —         | —           | User-defined |

New algorithm IDs (0x07–0x7F) are allocated by publishing a schema descriptor
of type `core/algorithm-registration` to the mesh. No governance body required —
the content-hash of the algorithm definition *is* the registration. Conflicts
are resolved by adoption (whichever ID gains critical mass wins).

Post-quantum algorithms (ML-DSA, ML-KEM) are reserved now and activated when
implementations mature. Nodes MUST accept descriptors signed with any algorithm
they understand and MUST ignore (not reject) descriptors signed with algorithms
they don't.

### 1.2 Hash Format

All hashes in the protocol use this structure:

```
Hash {
  algorithm:  u8        // from Algorithm Registry
  digest:     bytes     // length determined by algorithm
}
```

The **canonical hash algorithm** for content-addressing is BLAKE3 (0x03).
Nodes MUST support BLAKE3. Nodes MAY accept SHA-256 (0x04) for interop.

### 1.3 Identity Format

An identity is a public key with its algorithm tag:

```
Identity {
  algorithm:  u8        // signature algorithm from registry
  public_key: bytes     // raw public key bytes
}
```

The **DID** of an identity is derived deterministically:
```
did:mesh:<multibase-base58btc(algorithm_byte || public_key_bytes)>
```

This is a new DID method. The DID is self-certifying — you can derive the
public key from the DID and verify signatures without any resolver or network
lookup. Registration of the `did:mesh` method follows W3C DID Core conventions
but requires no DID registry. The method-specific identifier is the multibase
encoding of the algorithm-tagged public key.

### 1.4 Signature Envelope

Every signed structure in the protocol uses:

```
Signed<T> {
  payload:    bytes     // canonical CBOR serialization of T
  identity:   Identity  // signer's public key
  signature:  bytes     // signature over payload bytes
}
```

Verification: deserialize `identity`, verify `signature` over `payload` bytes
using the algorithm specified in `identity.algorithm`. If the algorithm is
unknown, the message is unverifiable (not invalid — skip, don't reject).

---

## 2. Descriptor Envelope

The descriptor is the fundamental unit of data on the mesh. Everything stored
on the DHT is a descriptor. Capabilities, schemas, revocations, attestations —
all use the same envelope.

```
Descriptor {
  // Immutable header (included in content hash)
  schema_hash:    Hash          // content-hash of the schema this payload conforms to
  topic:          string        // publisher-chosen descriptor slot (UTF-8, max 255 bytes)
  payload:        bytes         // opaque, interpreted according to schema

  // Publisher metadata (included in content hash)
  publisher:      Identity      // who published this
  timestamp:      u64           // microseconds since epoch
  sequence:       u64           // monotonic per publisher, for ordering
  ttl:            u32           // seconds until expiry

  // Routing (included in content hash)
  routing_keys:   [Hash]        // DHT key ranges where this should be stored (max 8)

  // Derived (not included in content hash)
  id:             Hash          // BLAKE3(canonical CBOR of all fields above)
  signature:      bytes         // sign(id.digest, publisher.private_key)
}
```

The `topic` field allows a publisher to maintain multiple active descriptors
under the same schema. For example, a provider advertising both text generation
and image generation publishes two `core/capability` descriptors with different
topics (e.g., `"text-generation"` and `"image-generation"`). The deduplication
key for descriptor replacement is `publisher + schema_hash + topic`. Topics
default to the empty string `""` for schemas where a publisher only ever needs
one active descriptor (e.g., `core/revocation`, `core/key-rotation`).

### 2.1 Content Addressing

The descriptor's `id` is computed by:

1. Serialize `schema_hash`, `topic`, `payload`, `publisher`, `timestamp`,
   `sequence`, `ttl`, and `routing_keys` as canonical CBOR (deterministic map
   key ordering).
2. Compute BLAKE3 hash of the serialized bytes.
3. The hash IS the descriptor ID.

This means:
- The same content always produces the same ID.
- Tampering with any field changes the ID.
- No central authority assigns IDs.
- Two publishers can independently verify they have the same descriptor.

### 2.2 Descriptor Validation

A node MUST validate before storing or forwarding a descriptor:

1. Recompute `id` from the declared fields. Reject if mismatch.
2. Verify `signature` against `publisher` identity and `id.digest`. Reject if
   invalid.
3. Check `timestamp + ttl` > now. Reject if expired.
4. Check `timestamp` ≤ now + 120 seconds. Reject if too far in the future.
   This prevents gaming TTL with future-dated timestamps. The 120-second
   tolerance accommodates clock skew without requiring NTP synchronization.
   Expiry is computed from `min(timestamp, now) + ttl` to prevent bonus TTL
   from borderline future timestamps.
5. Check `routing_keys` is non-empty and ≤ 8 entries. Reject if violated.
6. Check `payload` length ≤ 65,536 bytes. Reject if exceeded.
7. Check `topic` length ≤ 255 bytes. Reject if exceeded.
8. Check `sequence` ≥ any previously seen sequence for this `publisher` +
   `schema_hash` + `topic` combination. If lower, this is a stale
   descriptor — ignore.

The node MUST NOT validate the payload against the schema. The payload is
opaque to the protocol layer. Schema validation is the consumer's
responsibility.

### 2.3 Descriptor Republishing

Descriptors expire after `ttl` seconds. To remain discoverable, publishers
MUST re-publish before expiry. Best practice: re-publish at TTL/2 intervals.

When re-publishing, the publisher increments `sequence` and updates
`timestamp`. The `id` changes (because the content changed). DHT nodes
that receive a newer sequence for the same `publisher` + `schema_hash` +
`topic` combination replace the older version.

---

## 3. Wire Protocol

All communication between mesh nodes uses a binary framing protocol over QUIC
streams.

### 3.1 Connection Establishment

Nodes connect via QUIC (RFC 9000). The QUIC ALPN (Application-Layer Protocol
Negotiation) identifier is `mesh/0`.

Each QUIC connection supports multiple concurrent bidirectional streams.
Each stream carries exactly one request-response pair.

### 3.2 Message Frame

Every protocol message uses this frame:

```
Frame {
  magic:      u16       // 0x4D48 ("MH" — Mesh)
  version:    u8        // protocol version (0x01)
  msg_type:   u8        // message type (see below)
  msg_id:     [u8; 16]  // random request ID (for correlation)
  body_len:   u32       // length of body in bytes
  body:       bytes     // CBOR-encoded message body
}
```

Total header: 24 bytes. Minimal overhead.

### 3.3 Message Types

The protocol defines exactly 8 message types. Four request-response pairs.
This is the complete set. No extensions at this layer.

| Type | ID   | Direction | Description                        |
|------|------|-----------|------------------------------------|
| PING | 0x01 | Request   | Liveness check                     |
| PONG | 0x81 | Response  | Liveness confirmation              |
| STORE | 0x02 | Request  | Store a descriptor                 |
| STORE_ACK | 0x82 | Response | Acknowledge storage            |
| FIND_NODE | 0x03 | Request | Find nodes closest to a key     |
| FIND_NODE_RESULT | 0x83 | Response | Return closest nodes     |
| FIND_VALUE | 0x04 | Request | Find descriptors at a key       |
| FIND_VALUE_RESULT | 0x84 | Response | Return descriptors or closer nodes |

Request types have bit 7 clear. Response types have bit 7 set.
The response msg_type is always `request_msg_type | 0x80`.

### 3.4 PING / PONG

**PING (0x01):**
```cbor
{
  "sender": Identity,      // sender's identity
  "sender_addr": NodeAddr  // sender's QUIC endpoint
}
```

**PONG (0x81):**
```cbor
{
  "sender": Identity,
  "sender_addr": NodeAddr,
  "observed_addr": NodeAddr  // what the sender's address looks like from our side
}
```

`observed_addr` helps with NAT detection. If your observed address differs
from what you think it is, you're behind a NAT.

### 3.5 STORE / STORE_ACK

**STORE (0x02):**
```cbor
{
  "sender": Identity,
  "descriptor": Descriptor   // the full descriptor to store
}
```

**STORE_ACK (0x82):**
```cbor
{
  "stored": bool,            // whether the node accepted it
  "reason": string?          // if not stored, why (optional)
}
```

A node SHOULD store a descriptor if:
- It passes validation (Section 2.2).
- At least one of the descriptor's `routing_keys` falls within the node's
  responsible DHT range.
- The node has storage capacity.

A node MAY refuse storage for capacity reasons without penalty.

### 3.6 FIND_NODE / FIND_NODE_RESULT

**FIND_NODE (0x03):**
```cbor
{
  "sender": Identity,
  "target": Hash             // the DHT key to find nodes near
}
```

**FIND_NODE_RESULT (0x83):**
```cbor
{
  "nodes": [NodeInfo]        // up to K closest nodes (K=20 default)
}

NodeInfo {
  identity: Identity,
  addr: NodeAddr,
  last_seen: u64             // timestamp of last successful contact
}
```

Standard Kademlia node lookup. Returns the `K` nodes closest to `target`
from the responding node's routing table.

### 3.7 FIND_VALUE / FIND_VALUE_RESULT

**FIND_VALUE (0x04):**
```cbor
{
  "sender": Identity,
  "key": Hash,               // the routing key to search for
  "max_results": u16,        // max descriptors to return (default 20)
  "filters": FilterSet?      // optional payload-level filters (see 3.7.1)
}
```

**FIND_VALUE_RESULT (0x84):**
```cbor
{
  // Exactly one of these will be populated:
  "descriptors": [Descriptor]?,  // if this node has matching descriptors
  "nodes": [NodeInfo]?           // if not, closest nodes to continue searching
}
```

If the node has descriptors stored at `key`, it returns them. Otherwise it
returns the closest nodes it knows of, and the requester continues the
iterative lookup (standard Kademlia).

#### 3.7.1 Filters

Filters are optional hints that help nodes reduce response size. They
operate on descriptor metadata only (not payload content — the node
can't parse payloads).

```
FilterSet {
  schema_hash: Hash?,         // only return descriptors with this schema
  min_timestamp: u64?,        // only return descriptors newer than this
  publisher: Identity?,       // only return descriptors from this publisher
}
```

Filters are best-effort. A node MAY ignore filters and return all matching
descriptors. The requester MUST filter client-side regardless.

---

## 4. DHT Structure

### 4.1 Kademlia Parameters

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| K (bucket size) | 20 | Standard Kademlia. Balances routing table size vs lookup reliability. |
| α (concurrency) | 3 | Parallel lookups per iteration. |
| Key space | 256-bit | BLAKE3 output size. |
| Node ID | BLAKE3(public_key) | Deterministic, unforgeable. |
| Bucket refresh | 1 hour | Refresh any bucket not accessed in this interval. |
| Descriptor TTL | Publisher-defined | Min 60s, max 86400s (24h). Default 3600s (1h). |
| Republish interval | TTL/2 | Publisher re-stores before expiry. |
| Storage limit per node | Implementation-defined | Nodes set their own capacity. |

### 4.2 Node ID Derivation

```
node_id = BLAKE3(public_key_bytes)
```

The node ID is the hash of the raw public key. This means:
- Node IDs are uniformly distributed in the key space.
- You can't choose your node ID (no Sybil positioning without burning keys).
- You can verify a node's ID by asking for its public key and hashing it.

### 4.3 Routing Table

Standard Kademlia routing table: 256 k-buckets, one for each bit of XOR
distance from the local node ID. Each bucket holds up to K=20 node entries.

Node entries in a bucket are ordered by last-seen time (most recent last).
When a bucket is full and a new node is discovered:
1. PING the least-recently-seen node.
2. If it responds, move it to the end (most recent) and discard the new node.
3. If it doesn't respond, evict it and add the new node.

This gives preference to long-lived nodes, which empirically are more reliable.

### 4.4 Routing Key Computation

When a publisher wants their capability descriptor to be discoverable, they
compute routing keys that map their capability type to DHT key space:

```
routing_key = BLAKE3("mesh:route:" || capability_type_string)
```

For example:
```
routing_key = BLAKE3("mesh:route:compute/inference/text-generation")
```

A publisher MAY include multiple routing keys for the same descriptor if the
capability spans multiple categories.

A discoverer looking for capabilities of type `compute/inference/text-generation`
computes the same hash and performs FIND_VALUE at that key. The DHT converges
them.

### 4.5 Hierarchical Discovery

To support browsing ("show me all compute capabilities"), publishers SHOULD
include routing keys at multiple levels of their capability path:

```
routing_keys: [
  BLAKE3("mesh:route:compute"),
  BLAKE3("mesh:route:compute/inference"),
  BLAKE3("mesh:route:compute/inference/text-generation")
]
```

This allows both broad and narrow discovery. A query for `compute` returns
all compute-related descriptors. A query for `compute/inference/text-generation`
returns only text generation providers.

The `max 8 routing keys` limit prevents abuse while allowing reasonable
multi-category listing.

---

## 5. Core Schemas

These schemas are published to the mesh at bootstrap. They define the minimum
viable payload types for capability discovery.

### 5.1 Schema Descriptor Schema (self-describing root)

Schema hash: `BLAKE3("mesh:schema:core/schema")` (well-known, hardcoded)

This is the only hardcoded hash in the protocol. Everything else is derived.

```cbor
{
  "name": "core/schema",
  "version": 1,
  "format": "cbor-cddl",       // how to parse payloads using this schema
  "definition": "<CDDL text>", // the schema definition
  "extends": null,              // parent schema hash, if any
  "description": "Schema for schema descriptors"
}
```

### 5.2 Capability Advertisement Schema

Schema hash: `BLAKE3("mesh:schema:core/capability")` (well-known)

Payload structure:

```cddl
capability-advertisement = {
  type:          tstr,                    ; capability type path (e.g., "compute/inference/text-generation")
  version:       uint,                   ; schema version for this capability type
  params:        { * tstr => any },      ; capability-specific parameters (opaque to protocol)
  constraints:   ? constraints,          ; when/where/how this capability is available
  endpoint:      endpoint,               ; how to negotiate with this provider
  metadata:      ? { * tstr => any }     ; optional human/agent-readable metadata
}

constraints = {
  ? geo:         geo-constraint,         ; geographic bounds
  ? temporal:    temporal-constraint,    ; availability windows
  ? capacity:    capacity-constraint,    ; current load/limits
  ? pricing:     pricing-constraint      ; cost information
}

geo-constraint = {
  ? center:      [float, float],         ; [lat, lon]
  ? radius_km:   float,                  ; service radius
  ? regions:     [* tstr]                ; named regions (ISO 3166, free-form)
}

temporal-constraint = {
  ? available:   bool,                   ; is this available right now?
  ? windows:     [* time-window],        ; recurring availability
  ? until:       uint                    ; available until (timestamp)
}

time-window = {
  start: tstr,                           ; HH:MM
  end:   tstr,                           ; HH:MM
  days:  [* uint]                        ; 0=Mon, 6=Sun
  tz:    tstr                            ; IANA timezone
}

capacity-constraint = {
  ? current_load: float,                 ; 0.0–1.0 utilization
  ? max_concurrent: uint,               ; max parallel requests
  ? queue_depth: uint                    ; current queue depth
}

pricing-constraint = {
  ? model:       tstr,                   ; "per-request" | "per-second" | "per-token" | "per-unit" | "free"
  ? currency:    tstr,                   ; ISO 4217 or token symbol
  ? amount:      tstr,                   ; decimal string (to avoid float precision issues)
  ? details:     { * tstr => any }       ; pricing model specifics
}

endpoint = {
  protocol:      tstr,                   ; "mesh-negotiate" | "https" | "grpc" | "quic-stream" | other
  address:       tstr,                   ; protocol-specific address
  ? auth:        tstr                    ; auth method hint: "none" | "did-auth" | "bearer" | other
}
```

### 5.3 Discovery Query Schema

Schema hash: `BLAKE3("mesh:schema:core/discovery-query")` (well-known)

Discovery queries are NOT stored on the DHT. They are constructed locally
by the querying agent to determine which routing keys to FIND_VALUE for.
This schema exists for interop — if agents want to share or delegate
queries, they use this format.

```cddl
discovery-query = {
  type:          tstr,                   ; capability type path (exact or prefix)
  ? match:       tstr,                   ; "exact" | "prefix" (default: "exact")
  ? constraints: query-constraints,      ; desired constraints
  ? limit:       uint                    ; max results desired
}

query-constraints = {
  ? geo:         { ? center: [float, float], ? radius_km: float },
  ? min_capacity: float,                 ; minimum available capacity (0.0–1.0)
  ? max_price:   { currency: tstr, amount: tstr },
  ? available_now: bool                  ; must be currently available
}
```

### 5.4 Resolve Request/Response Schema

Schema hash: `BLAKE3("mesh:schema:core/resolve")` (well-known)

Resolve is a **direct peer-to-peer message**, not a DHT operation. After
discovering capability descriptors via the DHT, an agent may RESOLVE
directly with a provider to get real-time status.

Resolve uses a QUIC stream to the provider's endpoint address.

```cddl
resolve-request = {
  descriptor_id: bstr,                  ; the descriptor ID being resolved
  requester:     bstr,                  ; requester's DID
  ? intent:      { * tstr => any }      ; optional: what the requester wants to do
}

resolve-response = {
  status:        tstr,                  ; "available" | "busy" | "unavailable" | "moved"
  descriptor_id: bstr,                  ; echo back
  ? updated:     bstr,                  ; if "moved", new descriptor ID
  ? terms:       { * tstr => any },     ; current terms (pricing, ETA, constraints)
  ? challenge:   bstr                   ; optional DID-auth challenge for the requester
}
```

### 5.5 Revocation Schema

Schema hash: `BLAKE3("mesh:schema:core/revocation")` (well-known)

A revocation is a descriptor that cancels a previous descriptor. It's stored
on the DHT at the same routing keys as the original.

```cddl
revocation = {
  target_id:     bstr,                  ; descriptor ID being revoked
  reason:        tstr,                  ; "expired" | "superseded" | "compromised" | "withdrawn"
  ? successor:   bstr                   ; if superseded, the new descriptor ID
}
```

A revocation MUST be signed by the same identity that published the original
descriptor. Nodes that receive a valid revocation MUST remove the target
descriptor from storage.

Identity-level revocation (compromised key) is a separate mechanism — see
Section 7.

---

## 6. Bootstrap

### 6.1 Seed Nodes

A new node joining the mesh needs at least one known node to bootstrap its
routing table. The protocol does not mandate how seed nodes are discovered.

Recommended bootstrap methods (in preference order):

1. **Hardcoded seed list** — a small set of well-known, long-lived nodes
   operated by independent parties. Embedded in the node binary.
2. **DNS-based discovery** — SRV records at a well-known domain:
   `_mesh._quic.mesh.example.org`
3. **Local network discovery** — mDNS/DNS-SD for `_mesh._quic._local`
4. **Manual configuration** — user-provided node addresses.

Once a node has joined and populated its routing table, it no longer needs
seed nodes.

### 6.2 Core Schema Bootstrap

The 5 core schemas (Section 5.1–5.5) have well-known schema hashes derived
from well-known strings. Every node implementation MUST have these schemas
compiled in. They are also published to the mesh for discoverability, but
nodes MUST NOT depend on finding them via DHT lookup.

### 6.3 First Publish

A new node announces itself by:

1. Generate keypair → derive node ID.
2. Connect to a seed node.
3. Perform a FIND_NODE for its own node ID (populates routing table).
4. Publish any capability descriptors via STORE.

---

## 7. Identity Management

### 7.1 Key Rotation

An agent can rotate its identity key by publishing a **key rotation descriptor**:

Schema hash: `BLAKE3("mesh:schema:core/key-rotation")` (well-known)

```cddl
key-rotation = {
  old_identity:  bstr,                  ; old public key (Identity)
  new_identity:  bstr,                  ; new public key (Identity)
  effective:     uint,                  ; timestamp when rotation takes effect
  old_signature: bstr                   ; signature of new_identity by old key
}
```

This descriptor MUST be signed by the OLD key (proving ownership) and
the `old_signature` field contains a signature of the new identity by the
old key (double proof). Nodes receiving this update their routing tables.

After rotation, the old key is considered inactive. Descriptors signed by
the old key remain valid until their TTL expires, then are replaced by
descriptors signed by the new key.

### 7.2 Key Compromise

If a key is compromised, the holder publishes a key rotation AND a
revocation of all descriptors signed by the compromised key. If the
holder has lost access to the compromised key, they cannot do this —
this is an unsolvable problem without a higher authority.

For high-value identities, the recommended mitigation is **pre-committed
recovery keys**: at identity creation, the agent publishes a descriptor
containing the hash of a recovery key. If the primary key is compromised,
the recovery key can publish a revocation. This pattern is optional and
lives in Layer 3 (a schema, not a protocol feature).

### 7.3 Ephemeral Identities

Not all agents need persistent identities. A query-only agent can generate
a fresh keypair for each session. It can discover and resolve capabilities
without ever publishing anything. The protocol does not distinguish between
persistent and ephemeral identities — there's no "registration" step.

---

## 8. Transport Details

### 8.1 QUIC Configuration

| Parameter | Value | Rationale |
|-----------|-------|-----------|
| ALPN | `mesh/0` | Protocol identification |
| Max idle timeout | 30s | Keep connections alive for related queries |
| Initial RTT | 100ms | Conservative default |
| Max concurrent bidi streams | 100 | Parallel requests per connection |
| Max stream data | 1 MB | Limit per-stream memory |
| Keep-alive | 10s | Below NAT timeout thresholds |

### 8.2 Connection Multiplexing

A single QUIC connection between two nodes can carry multiple concurrent
protocol exchanges. Each exchange uses its own bidirectional stream:
one frame sent, one frame received, then the stream closes.

Nodes SHOULD reuse connections to peers they communicate with frequently.

### 8.3 NAT Traversal

QUIC's connection migration and the `observed_addr` field in PONG enable
basic NAT detection. For nodes behind restrictive NATs:

1. The node discovers its external address via PONG `observed_addr`.
2. The node uses QUIC's connection migration to maintain reachability.
3. For symmetric NATs, relay through a publicly-reachable node is the
   fallback. Relay is a voluntary service — any public node can offer it.
   The protocol for relay is a simple proxy: relay node accepts a QUIC
   connection, forwards streams to the target.

   Relay is **not part of this specification.** It is not needed until the
   mesh grows large enough that nodes behind restrictive NATs cannot find
   any publicly-reachable peer — a problem that does not exist in small or
   mid-sized networks where most nodes have direct connectivity. When relay
   becomes necessary, it will be defined as a schema (`core/relay`), not a
   core protocol feature. The anticipated design follows the libp2p Circuit
   Relay v2 pattern: relay nodes advertise via standard DHT descriptors
   (`infrastructure/relay`), NATed nodes establish reservations (long-lived
   QUIC streams to relay nodes), and connecting peers request relay via a
   handshake that results in dumb stream splicing. Abuse prevention uses
   duration caps, data caps, slot limits, and optional DID-auth. No new core
   message types are required — relay operates as a QUIC stream-level proxy
   using the existing transport.

---

## 9. Security Considerations

### 9.1 Sybil Resistance

An attacker can create many identities cheaply (just generate keypairs).
Protections:

- **Node ID = hash of public key** — an attacker can't choose their DHT
  position. Flooding a specific capability's routing keys requires brute-forcing
  hashes.
- **K-bucket preference for long-lived nodes** — Sybil nodes are new and
  get evicted in favor of established nodes.
- **Descriptors are signed** — Sybil nodes can store garbage but can't forge
  valid capability descriptors for other identities.
- **TTL-based expiry** — Sybil descriptors must be continuously refreshed,
  costing ongoing compute.

Additional Sybil mitigations (proof-of-work on STORE requests, stake
requirements for high-volume publishers, reputation-weighted routing) are
**not included in this specification.** The mitigations above are sufficient
for networks where the cost of generating keypairs is not worth the attack
value. These heavier mechanisms become relevant only if the mesh grows to a
scale where Sybil attacks have meaningful economic incentive — at that point
they can be introduced as Layer 2 schema extensions (see Section 14.1)
without protocol changes.

### 9.2 Eclipse Attacks

An attacker surrounding a target node with colluding nodes to control its
view of the network. Mitigations:

- **Diverse bootstrap** — connect to seed nodes from multiple independent
  operators.
- **Routing table diversity** — Kademlia's bucket structure naturally
  maintains connections across the full key space.
- **Out-of-band verification** — for critical operations, verify descriptor
  signatures against known-good identity keys obtained through a separate
  channel.

### 9.3 Descriptor Spam

An attacker flooding the DHT with garbage descriptors. Mitigations:

- **Payload size limit** (64 KB) — limits resource consumption per descriptor.
- **Routing key limit** (8) — limits amplification.
- **TTL minimum** (60s) — prevents rapid churn attacks.
- **Per-publisher rate limits** — nodes SHOULD limit STORE requests per
  identity per time period. Recommended: 10 descriptors per identity per
  minute.
- **Storage capacity limits** — nodes set their own storage limits and
  evict lowest-value descriptors (oldest, least-queried) when full.

### 9.4 Privacy

- **Discovery is pseudonymous** — agents are identified by public keys,
  not real-world identities.
- **FIND_VALUE reveals interest** — nodes along the lookup path can see
  what capability types you're searching for. For sensitive queries,
  use onion routing (defined in a schema extension, not core protocol).
- **Capability descriptors are public** — anything published to the mesh
  is visible to all participants. Do not publish sensitive information
  in descriptors.
- **QUIC encrypts transport** — all communication is encrypted in transit.
  Node operators cannot eavesdrop on other nodes' conversations.

---

## 10. Comparison with Existing Systems

| Property | Mesh Protocol | BitTorrent DHT | IPFS | DNS | WebMCP |
|----------|--------------|----------------|------|-----|--------|
| Data unit | Capability descriptor | Torrent info | Content block | Domain record | Web page |
| Addressing | Content-hash | Info-hash | Content-hash | Hierarchical names | URL |
| Discovery | DHT + capability routing | DHT + trackers | DHT + bitswap | Hierarchical resolvers | Web crawling |
| Identity | Self-certifying DID | None | PeerID (libp2p) | None | Domain certificates |
| Mutability | Versioned (sequence) | Immutable | IPNS (slow) | TTL-based | Mutable |
| Schema evolution | Content-addressed schemas | N/A | N/A | Record types | N/A |
| Signatures | Every descriptor signed | Torrent file only | Block-level CIDs | DNSSEC (optional) | TLS (transport only) |

---

## 11. Wire Examples

### 11.1 Publishing a Capability

Agent wants to advertise text generation inference:

```
1. Construct capability payload:
   {
     "type": "compute/inference/text-generation",
     "version": 1,
     "params": {
       "models": ["qwen3:8b", "llama-4-scout"],
       "max_context": 32768,
       "throughput_tps": 150
     },
     "constraints": {
       "capacity": { "current_load": 0.3, "max_concurrent": 10 },
       "pricing": { "model": "per-token", "currency": "USD", "amount": "0.0001" }
     },
     "endpoint": {
       "protocol": "mesh-negotiate",
       "address": "quic://198.51.100.42:4433",
       "auth": "did-auth"
     }
   }

2. Serialize payload as CBOR → payload_bytes

3. Construct descriptor:
   schema_hash:  BLAKE3("mesh:schema:core/capability")
   topic:        "text-generation"
   payload:      payload_bytes
   publisher:    { algorithm: 0x01, public_key: <32 bytes> }
   timestamp:    1774213800000000  (2026-03-22T17:30:00Z in microseconds)
   sequence:     1
   ttl:          3600
   routing_keys: [
     BLAKE3("mesh:route:compute"),
     BLAKE3("mesh:route:compute/inference"),
     BLAKE3("mesh:route:compute/inference/text-generation")
   ]

4. Compute id = BLAKE3(canonical CBOR of above fields)

5. Sign: signature = Ed25519.sign(private_key, id.digest)

6. STORE to DHT nodes responsible for each routing key.
```

### 11.2 Discovering a Capability

Agent wants to find text generation providers:

```
1. Compute routing key:
   key = BLAKE3("mesh:route:compute/inference/text-generation")

2. Iterative FIND_VALUE(key) through the DHT:
   a. Query α=3 closest known nodes.
   b. Each returns either descriptors or closer nodes.
   c. Continue until no closer nodes are returned.

3. Collect all returned descriptors.

4. For each descriptor:
   a. Verify id (recompute hash, check match).
   b. Verify signature (check against publisher identity).
   c. Check TTL (not expired).
   d. Parse payload using schema for core/capability.
   e. Apply local filters (pricing, capacity, location).

5. Rank results by local criteria.

6. RESOLVE with top candidates (direct QUIC connection to their endpoint).
```

---

## 12. Implementation Notes

### 12.1 Recommended Crate Structure (Rust)

```
mesh-protocol/
  mesh-core/          — types, serialization, hashing, signing
  mesh-dht/           — Kademlia implementation
  mesh-transport/     — QUIC transport (quinn-based)
  mesh-node/          — full node binary
  mesh-client/        — lightweight client library (discover + resolve, no DHT participation)
  mesh-schemas/       — core schema definitions and validators
```

### 12.2 Minimum Viable Implementation

For Phase 0 (two nodes on a LAN):

1. `mesh-core`: Identity generation, CBOR serialization, BLAKE3 hashing,
   Ed25519 signing/verification, descriptor creation/validation.
2. `mesh-transport`: QUIC connection setup, frame encoding/decoding.
3. `mesh-dht`: Single-bucket routing table (just a peer list), STORE and
   FIND_VALUE handling.
4. `mesh-node`: CLI binary that starts a node, publishes a test capability,
   discovers peers.

Skip for Phase 0: multi-bucket routing, republishing, NAT traversal,
key rotation, revocations, filters.

### 12.3 Interop Testing

Any two implementations that agree on:
1. CBOR canonical serialization
2. BLAKE3 hashing
3. Ed25519 signatures
4. The 8 message types
5. The 5 core schemas

...can interoperate. No negotiation step. No capability exchange.
The protocol version byte (`0x01`) is the compatibility marker.

---

## 13. Design Decisions (formerly Open Questions)

Items resolved during protocol design. Recorded here for rationale.

### 13.1 Capability Type Governance

**Decision: Fully emergent. No governance body.**

Capability type strings (e.g., `compute/inference/text-generation`) are free-form.
Anyone can publish any type string at any time. Discovery works when publishers
and consumers converge on the same strings. Market pressure is the curation
mechanism — if 80% of providers use one string, the rest adopt it or don't get
found.

Rationale: A curated taxonomy introduces a registry (violating principle #3) and
a bottleneck that cannot keep pace with daily capability emergence. Curation is
**not included in this specification** because the early network is small enough
that fragmentation is not a real problem — participants can coordinate on type
strings informally. If the mesh grows to a scale where fragmentation impairs
discovery, alias/curation schemas can be layered on via `core/schema` with
`extends`, requiring no protocol changes.

### 13.2 Incentive Alignment

**Decision: Protocol is incentive-agnostic. Market-driven.**

In the initial network, providers are the infrastructure. Running a node is the
cost of discoverability — providers store other agents' descriptors as a side
effect of DHT participation. Pure consumers use the lightweight client library
(`mesh-client`) and do not participate in storage.

At scale, infrastructure operators ("hubs") emerge with business models built on
top of the protocol: premium storage, guaranteed reachability, SLA-backed
routing. Hubs are the ISPs of the mesh. Their incentive is revenue from
providers and consumers who need reliable connectivity.

The protocol does not include an incentive mechanism because incentive structures
vary by deployment context and are better expressed as market relationships than
protocol features. Protocol-level incentive enforcement (proof-of-storage
challenges, reciprocity scoring) is **not included in this specification**
because the early network is self-incentivizing — providers run nodes to be
discovered, and that is sufficient motivation. These mechanisms become relevant
only if the mesh scales beyond the point where provider self-interest sustains
the DHT, and can be introduced as schema extensions without protocol changes.

### 13.3 Cross-Mesh Bridging

**Decision: Solved by existing primitives plus hub conventions.**

Kademlia has no concept of "separate networks" — there is one 256-bit key
space. If two clusters both use it, connecting a single bridge node merges them.
The existing bootstrap mechanisms (Section 6.1) are sufficient for initial
bridging.

At scale, hubs provide the backbone. Nodes operating as persistent hubs SHOULD
maintain a full peer list of all other hubs above a self-determined size
threshold. Hub discovery uses the same mesh primitives — hubs publish descriptors
under `infrastructure/hub` and query for peers at that routing key. Because hub
descriptors are small and hub counts are bounded (tens of thousands, not
millions), a full hub table is feasible on any modern hardware. This ensures
that connectivity to any single hub provides a path to the entire mesh.

### 13.4 Payload Encryption

**Decision: No encryption at the protocol layer.**

Descriptors are public. Everything published to the mesh is visible to all
participants. Access control is enforced at the endpoint/resolve layer — the
`endpoint.auth` field supports DID-auth, bearer tokens, and other mechanisms.
The existence of a capability is public; using it requires authentication.

Organizations requiring existence-level privacy (where even the presence of a
capability is sensitive) run private meshes with separate seed nodes and no
bridges to the public mesh. The protocol supports this without modification.

### 13.5 Versioned Protocol Upgrades

**Decision: Dual-stack operation. No coordination mechanism.**

When protocol version N+1 is defined, nodes upgrade at their own pace. Upgraded
nodes accept both version N and N+1 frames — the version byte in the frame
header tells the receiver how to parse. Once adoption of N+1 reaches critical
mass, nodes MAY drop version N support.

No flag day. No governance. If a v1-only node cannot communicate with a v2-only
node, they simply do not appear in each other's routing tables. The network
partitions by version, and any dual-stack node serves as a bridge.

In practice, the protocol's design philosophy ("the core never changes,
evolution happens in payload schemas") means version bumps should be
exceptionally rare. New capability types, schemas, and conventions are all
expressible without touching the wire protocol.

### 13.6 Legal Compliance

**Decision: Protocol is neutral infrastructure. Compliance is the operator's responsibility.**

Like TCP/IP, the protocol does not know or care what flows over it. Regulatory
compliance — licensing, jurisdictional restrictions, certification requirements —
is the responsibility of the entities operating nodes and consuming capabilities.

The existing `constraints` and `metadata` fields in the capability advertisement
schema (Section 5.2) are sufficient for providers to self-declare regulatory
information (jurisdictions, licenses, certifications). No protocol changes are
needed. Formal compliance guidance is **not included in this specification** because
the mesh does not yet operate in regulated industries. When enterprise
adoption creates demand for standardized compliance metadata, guidance may
be published as a supplementary document. The protocol will not need to
change — the existing schema fields are sufficient to carry any compliance
information the market requires.

---

## 14. Protocol Evolution

This protocol is designed to outlast its creators (Section 0). That means it
must evolve without central coordination. This section defines how changes
happen at each layer of the system.

### 14.1 Evolution Layers

Changes to the mesh fall into three distinct layers, each with a different
adoption model:

**Layer 1: Wire Protocol**

Changes to the frame format (Section 3.2), message types (Section 3.3), or
fundamental conventions (Section 1). These are the rarest and most disruptive
changes. A wire protocol change requires a version bump — the `version` byte
in the frame header increments from `0x01` to `0x02`.

Adoption model: **physical incompatibility.** Old nodes cannot parse new frames.
Upgraded nodes SHOULD accept both the current and previous protocol versions
(dual-stack operation). The version byte tells the receiver how to parse.
During transition, dual-stack nodes serve as bridges between version clusters.
Once the old version's adoption falls below a viable threshold, nodes MAY drop
support for it. There is no flag day and no coordinating authority.

The protocol's design philosophy — "the core never changes, evolution happens
in payload schemas" — means Layer 1 changes should be exceptionally rare. If
the core is designed correctly, they may never be needed.

**Layer 2: Schemas**

New payload schemas, new fields in existing schemas, new conventions for
capability types. These are the primary mechanism for protocol evolution. A
schema change requires no protocol modification — it is simply a new descriptor
published to the mesh.

Adoption model: **consumer choice.** Nodes that understand a new schema use it.
Nodes that don't ignore it. If consumers begin filtering for a new schema field
(e.g., compliance metadata), providers adopt it or lose visibility. The protocol
layer is unaware of schema evolution — it stores and routes descriptors
regardless of their payload content (Section 2.2: "the node MUST NOT validate
the payload against the schema").

Examples of Layer 2 evolution:
- Adding a `core/compliance` schema for regulatory metadata
- Adding a `core/type-alias` schema for capability taxonomy convergence
- Adding a `core/relay` schema for NAT traversal relay (Section 8.3) — not
  needed until the mesh reaches a scale where NATed nodes cannot find any
  directly-reachable peer
- Adding a notification/subscription overlay for real-time capability updates
  — not needed while descriptor TTLs are short and poll frequency is
  manageable; becomes relevant when consumers need sub-second awareness of
  capability changes (the proven pattern is announce-then-fetch: push a
  lightweight announcement, let receivers pull the full descriptor via
  FIND_VALUE, as used by IPFS PubSub and Ethereum devp2p)
- Extending capability constraints with new fields (e.g., latency SLAs)
- Defining industry-specific capability schemas (healthcare, finance)

**Layer 3: Behavioral Conventions**

Operating practices, recommended configurations, hub policies. These are
SHOULD-level guidance in the spec or in supplementary documents. They are
never enforced by the protocol.

Adoption model: **social and market pressure.** Hubs that follow best practices
attract more traffic. Nodes that misbehave get routed around. No node is
dropped or rejected for ignoring a convention — it simply becomes less useful
to the network.

Examples of Layer 3 evolution:
- Hub operators maintaining full hub peer lists (Section 13.3)
- Rate limiting recommendations (Section 9.3)
- Republishing intervals (Section 2.3)
- Industry-specific compliance postures

### 14.2 The Protocol Does Not Enforce Policy

The protocol is neutral infrastructure. It does not understand compliance,
content policy, capability quality, or business rules. It transports signed
descriptors and routes queries. Nothing more.

Policy enforcement happens at higher layers:
- **Consumers** filter descriptors based on their own criteria (compliance,
  pricing, reputation, schema fields).
- **Hubs** set their own storage and routing policies as business decisions.
  A hub serving regulated industries may refuse to cache descriptors lacking
  compliance metadata. This is the hub's policy, not the protocol's.
- **Providers** self-declare regulatory information using schema fields
  (`constraints`, `metadata`). The protocol carries this information but
  does not validate or enforce it.

This is the same model as the internet. TCP/IP does not care what flows over
it. ISPs, CDNs, and browsers make policy decisions at higher layers. The mesh
protocol is the TCP/IP. Hubs are the ISPs. Consumer agents are the browsers.

### 14.3 Change Process

There is no formal governance body for the protocol. Changes follow the same
pattern as the capability taxonomy (Section 13.1): emergent, adoption-driven.

For Layer 2 and Layer 3 changes:
1. Anyone publishes a new schema or proposes a convention.
2. Implementations adopt it (or don't).
3. Whichever approach gains critical mass becomes the de facto standard.
4. The spec MAY be updated to document conventions that have achieved wide
   adoption.

For Layer 1 changes (if ever needed):
1. A new protocol version is proposed with a specification document.
2. Implementations ship dual-stack support.
3. The network transitions organically as nodes upgrade.
4. No node is forced to upgrade. Nodes on the old version continue operating
   in their version cluster until they choose to upgrade or go offline.

---

## Appendix A: CBOR Tag Assignments

| Tag | Semantics |
|-----|-----------|
| 42  | Mesh Identity (algorithm + public_key) |
| 43  | Mesh Hash (algorithm + digest) |
| 44  | Mesh Descriptor |
| 45  | Mesh Frame |

Standard CBOR tags (RFC 8949) are used for dates, big integers, etc.

## Appendix B: Well-Known Schema Hashes

Computed as `BLAKE3("mesh:schema:<name>")`:

| Schema | Input String | Hash (hex, first 16 bytes shown) |
|--------|-------------|----------------------------------|
| core/schema | `mesh:schema:core/schema` | *(compute at implementation time)* |
| core/capability | `mesh:schema:core/capability` | *(compute at implementation time)* |
| core/discovery-query | `mesh:schema:core/discovery-query` | *(compute at implementation time)* |
| core/resolve | `mesh:schema:core/resolve` | *(compute at implementation time)* |
| core/revocation | `mesh:schema:core/revocation` | *(compute at implementation time)* |
| core/key-rotation | `mesh:schema:core/key-rotation` | *(compute at implementation time)* |

## Appendix C: Canonical Serialization & Test Vectors

### C.1 Canonical Hash Input Format

The canonical hash input for computing a descriptor's content ID is a **CBOR map
(major type 5)** with string keys sorted in lexicographic (byte) order. Each key
is a CBOR text string (major type 3). Values use the exact CBOR types specified
below.

This canonical form is used **only** for computing the descriptor ID hash. It is
**not** the wire format for network serialization (which may use any valid CBOR
encoding, including arrays or different key orderings).

### C.2 Field Types

| Key (text string) | CBOR Type | Description |
|---|---|---|
| `payload` | byte string (major type 2) | Raw payload bytes |
| `publisher` | array: [unsigned integer (algo), byte string (pubkey)] | Publisher identity |
| `routing_keys` | array of arrays: [[unsigned integer (algo), byte string (digest)], ...] | DHT routing keys |
| `schema_hash` | array: [unsigned integer (algo), byte string (digest)] | Schema content hash |
| `sequence` | unsigned integer (major type 0) | Monotonic sequence number |
| `timestamp` | unsigned integer (major type 0) | Microseconds since Unix epoch |
| `topic` | text string (major type 3) | Publisher-chosen topic |
| `ttl` | unsigned integer (major type 0) | Time-to-live in seconds |

Keys MUST appear in the map in the order shown above (which is lexicographic).

### C.3 Test Vector

**Inputs:**

| Field | Value |
|---|---|
| Secret key (Ed25519) | `0101010101010101010101010101010101010101010101010101010101010101` |
| publisher.algorithm | `0x01` (Ed25519) |
| publisher.public_key | `8a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c` |
| schema_hash.algorithm | `0x03` (BLAKE3) |
| schema_hash.digest | `bd40bb81f07d1e149cc709b581a4c52af445f6a203d7ab32e284a0b3ffcfb330` |
| topic | `test-topic` |
| payload | `74657374207061796c6f6164` (ASCII: "test payload") |
| timestamp | `1700000000000000` (microseconds) |
| sequence | `1` |
| ttl | `3600` |
| routing_keys[0].algorithm | `0x03` (BLAKE3) |
| routing_keys[0].digest | `eea1159ae33052b4a1c6e6cd41b1c923642fd9879aebb2b875458d785b9ae4f5` |

**Schema hash** is computed as `BLAKE3("mesh:schema:core/capability")`.
**Routing key** is computed as `BLAKE3("mesh:routing:compute/inference/text-generation")`.

**Canonical CBOR (hex, 219 bytes):**

```
a8677061796c6f61644c74657374207061796c6f6164697075626c697368657282
0158208a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f
6f5c6c726f7574696e675f6b6579738182035820eea1159ae33052b4a1c6e6cd41
b1c923642fd9879aebb2b875458d785b9ae4f56b736368656d615f686173688203
5820bd40bb81f07d1e149cc709b581a4c52af445f6a203d7ab32e284a0b3ffcfb3
306873657175656e6365016974696d657374616d701b00060a24181e400065746f
7069636a746573742d746f7069636374746c190e10
```

**BLAKE3 descriptor ID (hex):**

```
321631d68f034cbdb122eaaaefe9216370f28020900239dfcdbefa66c14df507
```

### C.4 Conformance

Implementations **MUST** produce byte-identical canonical CBOR for the same input
fields. The test vector in this appendix is the conformance test.

If your implementation produces the BLAKE3 hash
`321631d68f034cbdb122eaaaefe9216370f28020900239dfcdbefa66c14df507` for the
test vector inputs above, your canonical serialization is correct.

---

*This document is the protocol. The protocol is this document.*
*Build it. Break it. Ship it.*
