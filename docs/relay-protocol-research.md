# Relay Protocol Research — Findings & Recommendation

**Date:** 2026-03-22
**Context:** Capability Mesh Protocol (QUIC transport, Kademlia routing)
**Question:** How should `core/relay` work?

---

## 1. Survey of Existing Systems

### 1.1 libp2p Circuit Relay v2

**How it works:**
- Split into two sub-protocols: **hop** (client-to-relay) and **stop** (relay-to-destination).
- **Reservation model:** A NATed node (A) opens a persistent connection to relay (R) and sends `RESERVE`. R responds with `STATUS:OK` + a signed **Reservation Voucher** (cryptographic proof R will relay for A). A refreshes before expiry.
- **Connection initiation:** Peer B sends `CONNECT(target=A)` to R via hop protocol. R forwards a `CONNECT(from=B)` to A via stop protocol over the existing reservation connection. A responds `STATUS:OK`. R then splices the two streams — raw bidirectional byte forwarding.
- **Relay discovery:** No special mechanism. Relays are found through the DHT or preconfigured. Nodes detect they need relay via AutoNAT (peers tell you whether you're reachable).
- **Abuse prevention:** v2's key innovation — **limited relays**. Each reservation has a duration cap and data cap (e.g., 2 minutes, 128KB). This is enough for hole-punching coordination, not for bulk data. Relays also limit total reservation slots and use ACLs (`PERMISSION_DENIED`).
- **Vouchers:** Signed envelopes (`libp2p-relay-rsvp`) containing relay PeerID, reserving PeerID, expiration. Intended to eventually be required when dialing relay addresses.

**Key takeaway:** The simplest viable design. Reservation + limited relay + stream splicing. No new frame types needed beyond the hop/stop negotiation.

### 1.2 TURN (RFC 5766)

**How it works:**
- Client sends `Allocate` request to TURN server. Server allocates a **relayed transport address** (public IP:port) and returns it.
- Client installs **permissions** (IP allowlist) via `CreatePermission`. Only permitted peers can send data through the relay.
- Data forwarding: two modes — `Send/Data` indications (with full headers, ~36 byte overhead per packet) or **ChannelBind** (binds a peer to a 4-byte channel number, only 4 bytes overhead).
- Allocations expire after 10 minutes, refreshed via `Refresh` requests.
- **Authentication:** Long-term credentials (username + HMAC key). Every request (except indications) must be authenticated.
- **Abuse prevention:** Per-allocation bandwidth isn't explicitly capped in the RFC, but servers enforce limits. Authentication is mandatory. Permissions are explicit — you must allowlist each peer IP.

**Key takeaway:** TURN is the gold standard for relay in production (used by every WebRTC call). ChannelBind's 4-byte overhead is the benchmark for minimal relay framing. The permission model (explicit allowlisting) is worth adopting.

### 1.3 Tor Relay Circuits

**How it works:**
- Client picks a chain of 3 onion routers. Opens TLS to first, sends `CREATE2` cell with Diffie-Hellman handshake. Then sends `EXTEND2` relay cells (encrypted in layers) to build through each hop.
- Each hop only knows previous and next hop. **Full onion encryption** — each relay peels one layer.
- Relay discovery: Consensus document from directory authorities lists all relays with their keys, bandwidth, flags.

**Key takeaway:** Overkill for our use case. The circuit-building protocol is complex (multi-hop, layered encryption, directory authority consensus). However, the concept of relays advertising themselves in a directory maps well to DHT-based relay discovery.

### 1.4 I2P Tunnel Model

**How it works:**
- **Unidirectional tunnels** (unlike Tor's bidirectional circuits). Alice builds an outbound tunnel (A->B->C), Bob builds an inbound tunnel (D->E->Bob). Alice sends through her outbound tunnel to Bob's inbound gateway.
- **Garlic routing:** Multiple messages bundled into one encrypted "clove" — reduces traffic analysis.
- Tunnel build uses an encrypted build request passed through the tunnel itself, each hop decrypts its layer and decides whether to participate.
- Tunnels are rebuilt every 10 minutes.

**Key takeaway:** Unidirectional tunnels are an interesting design but add complexity. The garlic bundling idea is irrelevant for our case. I2P's model is heavier than what we need.

### 1.5 Meshtastic Relay/Repeater

**How it works:**
- **Managed flooding** for broadcasts: every node rebroadcasts received packets, decrementing a `HopLimit`. Before rebroadcasting, nodes listen briefly — if someone else already rebroadcast, they skip. Nodes farther away (lower SNR) get priority via smaller contention windows.
- `ROUTER` and `REPEATER` roles rebroadcast unconditionally and with higher priority.
- **Direct messages (since v2.6):** Uses **next-hop routing**. First message floods to find destination. On successful delivery, the relay path is recorded. Subsequent messages use the learned next-hop. Falls back to flooding if next-hop becomes unreachable.
- Packet header includes `next-hop` and `relay-node` fields (1 byte each).

**Key takeaway:** The next-hop learning pattern is elegant for low-bandwidth environments. The hop-limit approach (TTL-style) could be useful for relay chain depth limiting.

### 1.6 QUIC-Specific Relay Approaches

**QUIC connection migration** allows a connection to survive IP address changes (NAT rebinding), but this doesn't help when a node is completely unreachable from the outside.

**MASQUE (RFC 9298):** The IETF's answer to QUIC-aware proxying. `CONNECT-UDP` (HTTP/3 extended CONNECT) sets up a UDP tunnel through an HTTP/3 proxy. The proxy forwards UDP datagrams between client and target. This is essentially TURN reimagined for HTTP/3.

**Key takeaway:** No QUIC-native relay protocol exists. MASQUE is the closest, but it's HTTP/3-based and heavyweight for a minimal mesh protocol. The practical answer: relay at the QUIC-stream level (splice bidirectional streams), not at the UDP datagram level. QUIC gives you multiplexed streams for free — relay becomes stream forwarding.

---

## 2. Comparative Summary

| Aspect | libp2p v2 | TURN | Tor | I2P | Meshtastic | MASQUE |
|--------|-----------|------|-----|-----|------------|--------|
| Relay discovery | DHT/preconfigured | Configured server | Directory authority | NetDB | Role-based (ROUTER) | Configured proxy |
| Reservation | Explicit (RESERVE) | Allocate request | Circuit build | Tunnel build | None (always relay) | HTTP CONNECT |
| Data forwarding | Stream splice | Send/Channel | Onion cells | Garlic messages | Packet rebroadcast | UDP datagram proxy |
| Abuse prevention | Duration+data cap, slots | Auth+permissions | Bandwidth authority | Tunnel rebuild | Hop limit | HTTP auth |
| Overhead | ~0 (raw splice) | 4-36 bytes/pkt | 512-byte cells | 1KB messages | 2 bytes (headers) | HTTP framing |
| Complexity | Low | Medium | Very high | High | Low | Medium |

---

## 3. Practical Recommendation for Mesh Protocol

### 3.1 Design: libp2p v2-inspired, TURN-hardened

The protocol already says relay is "a simple proxy: relay node accepts a QUIC connection, forwards streams to the target. This is defined in a schema (`core/relay`), not in the core protocol." This is the right instinct. Here's how to make it concrete:

### 3.2 The `core/relay` Schema

**Three operations, all over normal QUIC streams using existing message framing:**

#### Operation 1: Relay Advertisement (DHT descriptor)
A public node that offers relay publishes a descriptor with schema `core/relay`:

```cddl
relay-advertisement = {
  max_reservations:  uint,          ; how many peers this relay supports
  max_duration_s:    uint,          ; max seconds per relayed connection
  max_data_bytes:    uint,          ; max bytes per relayed connection
  max_concurrent:    uint,          ; max concurrent connections per reservation
  requires_auth:     bool,          ; whether relay requires DID-auth
  ? allowed_schemas: [+ bstr],     ; optional: only relay for nodes advertising these capability schemas
}
```

Discovery: nodes find relays by querying the DHT for `BLAKE3("mesh:schema:core/relay")`. Same hierarchical discovery as capabilities.

#### Operation 2: Reservation (long-lived QUIC stream)

NATed node A opens a QUIC connection to relay R and opens a **reservation stream** (a long-lived bidirectional stream, unlike the normal request-response pattern):

```
A -> R:  { "op": "reserve", "ttl": 3600 }
R -> A:  { "op": "reserved", "expire": <timestamp>, "relay_addr": <R's public addr>,
           "voucher": <signed(R.key, {relay: R.id, peer: A.id, expire: timestamp})> }
```

A keeps this stream open. R now knows that A is reachable via this connection. A advertises `R's address + /relay/ + A's node ID` as a reachable address in its capability descriptors.

The voucher is a standard Descriptor signed by R, using a `core/relay-voucher` schema. It can be verified by any peer without contacting R.

#### Operation 3: Relay Connect (new stream on existing connection)

When B wants to reach A through R:

```
B -> R:  { "op": "connect", "target": <A.node_id>, "voucher_id": <optional> }
R -> A:  { "op": "incoming", "from": <B.node_id>, "from_addr": <B's observed addr> }
A -> R:  { "op": "accept" }  (or { "op": "reject", "reason": "..." })
R -> B:  { "op": "connected" }
```

After this handshake, R **splices the two streams**: every byte B writes is forwarded to A and vice versa. B and A then perform a normal mesh protocol exchange (including their own QUIC-level crypto if desired, since the relay content is opaque bytes to R).

### 3.3 Abuse Prevention

Drawn from the best of each system:

| Mechanism | Inspired by | How |
|-----------|------------|-----|
| **Duration + data caps** | libp2p v2 | Each relayed connection has a max duration (e.g., 120s) and max bytes (e.g., 1MB). Advertised in relay descriptor. Relay kills connection at limit. |
| **Reservation slots** | libp2p v2 | Relay limits total reservations (e.g., 50). Returns `RESERVATION_FULL` when at capacity. |
| **Explicit permissions** | TURN | A can optionally specify an allowlist of node IDs that may connect through R. |
| **Authentication** | TURN | Relay MAY require DID-auth (mutual TLS or signed challenge) before accepting reservations. |
| **Hop limit** | Meshtastic | Relay-through-relay (chaining) uses a TTL field, max depth 2. Prevents infinite relay chains. |
| **Rate limiting** | Common practice | Relay limits new connections per reservation per minute. |

### 3.4 Can This Be a Thin Proxy Layer?

**Yes.** This design requires:
- **Zero new core protocol message types.** All relay operations use the existing CBOR-over-QUIC-stream message frame (Section 3.2 of the spec). The `msg_type` can be a single new value (e.g., `0x05 RELAY`) or, even simpler, relay ops are just CBOR payloads on streams opened to a relay node, distinguished by the `core/relay` schema context.
- **No changes to framing, DHT, or descriptors.** Relay advertisement is a normal descriptor. Relay operations are normal QUIC streams.
- **The relay node is dumb.** After the connect handshake, it byte-splices two streams. It doesn't parse, validate, or inspect forwarded content. This is critical for keeping relay implementation simple.

### 3.5 Implementation Phases

**Phase 0 (minimum viable relay):**
- Single relay node, manually configured.
- Reserve + connect + stream splice.
- Fixed limits (120s, 1MB).
- No vouchers, no auth.

**Phase 1 (discoverable relay):**
- Relay advertisement descriptors on DHT.
- AutoNAT-style detection (use PONG `observed_addr` mismatch).
- Vouchers for relay address verification.

**Phase 2 (hardened relay):**
- DID-auth for reservations.
- Permission allowlists.
- Relay chaining with TTL.
- Hole-punch coordination: relay used as signaling channel to attempt direct QUIC connection via address exchange.

### 3.6 What NOT to Do

1. **Don't build onion routing into relay.** Tor/I2P's multi-hop encrypted tunnels are for anonymity, not NAT traversal. If anonymity is needed later, it's a separate schema extension (the spec already notes this).
2. **Don't proxy at the UDP level.** MASQUE/TURN operate on datagrams. The mesh protocol operates on QUIC streams. Relay at the stream level — it's simpler and leverages QUIC's built-in congestion control and flow control.
3. **Don't make relay mandatory.** It's a capability that public nodes voluntarily offer. NATed nodes that can hole-punch don't need it.
4. **Don't invent a new framing format.** Use the existing CBOR message frame. Relay ops are just another payload schema.

---

## 4. Open Questions

1. **Should relay nodes charge for service?** The protocol supports capability negotiation — a `core/relay` descriptor could include pricing terms, resolved via `core/resolve`. Defer to economic layer.
2. **Should vouchers be required?** libp2p v2 doesn't enforce them yet. Start without, add enforcement in Phase 2 if relay abuse emerges.
3. **Should the relay see node IDs?** Currently yes (needed for routing). For privacy, a blinded-ID scheme could be added later without changing the relay wire protocol.
4. **Connection upgrade after relay establishment?** B and A should attempt a direct QUIC connection (using addresses exchanged through the relay) and migrate away from the relay. This is the hole-punching coordination pattern from libp2p. Define in Phase 2.
