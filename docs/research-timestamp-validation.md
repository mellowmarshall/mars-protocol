# Timestamp Validation in Distributed Systems — Research Findings

**Date:** 2026-03-22
**Purpose:** Inform clock-skew and future-timestamp policy for the Capability Mesh Protocol

---

## 1. Kademlia-Based Systems (BitTorrent DHT, IPFS/libp2p)

### BitTorrent DHT (BEP-44 Mutable Items)
- **No wall-clock timestamps at all.** Mutable DHT items use a monotonic **sequence number** (int64) instead of timestamps.
- Storage nodes reject puts where `seq` is less than the currently stored sequence number (error 302).
- Freshness is enforced by **republish obligation** — items that aren't periodically re-put simply expire from storage nodes' caches.
- CAS (compare-and-swap) with sequence numbers prevents race conditions between concurrent writers.

### IPFS / libp2p Kademlia DHT
- **IPNS records use an explicit expiry timestamp** (`Validity` field, RFC 3339 with nanosecond precision) combined with a `Sequence` number.
- The `ValidityType = EOL` (End of Life) pattern: the record declares "I am valid until time T."
- DHT peers only keep records for up to **48 hours** regardless of declared expiry — the storage layer imposes its own ceiling.
- Record validation is pluggable via a `Validator` interface with `Validate()` (checks signature + expiry) and `Select()` (picks best record among candidates, typically highest sequence number wins).
- **TTL field** (nanoseconds) is a cache hint separate from the hard expiry — analogous to DNS TTL.
- **No explicit future-timestamp rejection** in the spec; however, `Select()` always prefers higher sequence numbers, so a future-dated record with a lower sequence number loses. The expiry is checked against the *receiving node's local clock*.

### Key Takeaway
P2P DHT systems avoid the future-timestamp problem by either (a) not using wall-clock timestamps at all (BitTorrent), or (b) using timestamps only for expiry-in-the-past checks, combined with monotonic sequence numbers for ordering (IPNS).

---

## 2. Distributed Databases

### CockroachDB
- Uses **Hybrid Logical Clocks (HLC)**: a physical component (wall time) + a logical counter.
- **Default max clock offset: 500ms.** Nodes crash if they detect skew exceeding 80% of this threshold with a majority of peers.
- **NTP is mandatory** for production deployments. The system trusts but verifies wall-clock synchronization.
- When reading, CockroachDB uses an **uncertainty interval** `[timestamp, timestamp + max_offset]`. If a read encounters a value in its uncertainty window, the transaction restarts at a higher timestamp. This is the cost of not having TrueTime — reads may occasionally retry.
- **Commit-wait** (Spanner-style) is available for non-blocking transactions: writer waits until HLC advances past commit timestamp.
- **Future writes are not rejected** — they are handled via the uncertainty interval mechanism. But a node whose clock is too far ahead will be killed by the offset enforcement.

### Cassandra
- Uses **wall-clock timestamps with microsecond resolution** for conflict resolution (Last-Write-Wins / LWW).
- **No built-in clock skew detection or enforcement.** Cassandra trusts NTP and assumes clocks are "close enough."
- A node with a clock running fast will win all LWW conflicts, silently overwriting legitimate data.
- This is a well-known operational hazard — Cassandra's documentation and community strongly recommend NTP but the system itself has no guardrails.

### Key Takeaway
Serious distributed databases either enforce clock bounds aggressively (CockroachDB: crash on skew > 400ms) or accept the risk (Cassandra). HLCs are the state-of-the-art for combining physical time with causal ordering.

---

## 3. Blockchain Systems

### Bitcoin
- Block timestamp must be **greater than the median of the previous 11 blocks** (lower bound).
- Block timestamp must be **less than network-adjusted time + 2 hours** (upper bound / future limit).
- "Network-adjusted time" = local UTC + median offset from all connected peers, capped at **+/- 70 minutes** from local system time.
- **Tolerance window: 2 hours into the future.**

### Ethereum (Pre-Merge / PoW)
- Block timestamp must be **greater than parent block timestamp**.
- Validators reject blocks with timestamps more than **15 seconds** in the future.

### Ethereum (Post-Merge / Beacon Chain)
- Time is slot-based: **12-second slots**, timestamps are deterministic (genesis_time + slot * 12).
- No clock-skew tolerance needed — timestamps are derived from slot numbers, not wall clocks.

### Key Takeaway
Blockchains use a two-sided bound: past limit (must advance) + future limit (not too far ahead). Bitcoin tolerates 2 hours; Ethereum PoW was 15 seconds. Post-merge Ethereum eliminated the problem entirely by making timestamps deterministic from slot numbers.

---

## 4. Summary of Approaches

| System | Time Source | Future Tolerance | Ordering Mechanism | Skew Enforcement |
|--------|-----------|-----------------|-------------------|-----------------|
| BitTorrent DHT | None (no timestamps) | N/A | Sequence numbers | N/A |
| IPNS | Wall clock (expiry only) | None explicit | Sequence numbers | Expiry checked against local clock |
| CockroachDB | HLC (wall + logical) | 500ms (uncertainty interval) | HLC timestamps | Crash at 80% of max-offset |
| Cassandra | Wall clock (microseconds) | None | LWW timestamps | None (trust NTP) |
| Bitcoin | Network-adjusted time | 2 hours | Chain height | Median-of-11 + peer time |
| Ethereum PoW | Wall clock | 15 seconds | Block number | Parent timestamp |
| Ethereum PoS | Deterministic slots | 0 (deterministic) | Slot number | Slot-based |

---

## 5. Approaches Beyond Simple Clock-Skew Tolerance

1. **Hybrid Logical Clocks (HLC):** Physical time + logical counter. Provides causal ordering while staying close to wall time. Used by CockroachDB. The HLC paper: Kulkarni et al., 2014 (Buffalo TR 2014-04).

2. **Sequence Numbers (monotonic counters):** Sidestep wall-clock entirely. BitTorrent BEP-44, IPNS. Simple, robust, but require the publisher to maintain state.

3. **Network-Adjusted Time:** Bitcoin's approach — compute median offset from peers to derive a shared "network time." Resistant to individual bad clocks but still has a tolerance window.

4. **Slot/Epoch-Based Time:** Ethereum PoS. Discretize time into slots. Deterministic, no skew, but requires consensus on slot boundaries.

5. **Commit-Wait:** Google Spanner / CockroachDB non-blocking transactions. After writing, wait for max-offset to elapse before declaring the transaction committed. Guarantees external consistency at the cost of latency.

6. **Expiry-Only Timestamps (EOL pattern):** IPNS. Timestamps are only used to say "this record is no longer valid after T." Ordering uses a separate sequence number. This decouples freshness from ordering.

---

## 6. Recommendation for Capability Mesh Protocol

The Mesh protocol uses **TTL-based descriptor expiry with microsecond timestamps** (Section 5 of PROTOCOL.md: `published_at` u64 + `ttl_seconds` u32). The relevant question is: how should receiving nodes handle descriptors whose `published_at` is in the future relative to the receiver's local clock?

### Recommended Design

**Adopt the IPNS/BEP-44 hybrid pattern with a Bitcoin-style future guard:**

1. **Use `published_at` only for expiry calculation, not ordering.**
   - A descriptor is expired when `now > published_at + (ttl_seconds * 1_000_000)`.
   - Ordering between versions of the same descriptor should use a **monotonic sequence number** (already present as `seq` in the descriptor schema) or compare `published_at` only as a tiebreaker.

2. **Reject descriptors with `published_at` too far in the future.**
   - Define a protocol-level constant: `MAX_FUTURE_SKEW = 120_000_000` (120 seconds in microseconds).
   - Any descriptor where `published_at > local_now + MAX_FUTURE_SKEW` is rejected with a validation error.
   - **Why 120 seconds:** This is generous enough to accommodate NTP-synced hosts (typical skew < 100ms), hosts with degraded NTP (skew up to ~10s), and even hosts with no NTP but reasonable hardware clocks (drift ~1s/day). It is far tighter than Bitcoin's 2 hours but looser than Ethereum's 15 seconds — appropriate for a protocol that doesn't have block-rate time pressure. It matches the order of magnitude used in practice by systems like Kerberos (5-minute tolerance) while being tighter because microsecond timestamps imply the publisher cares about precision.

3. **Clamp borderline future timestamps rather than rejecting.**
   - If `published_at` is in the future but within `MAX_FUTURE_SKEW`, the receiving node should **accept the descriptor but calculate expiry from its own `local_now`** rather than trusting the future `published_at`. This prevents a slightly-ahead publisher from getting bonus TTL.
   - Formula: `effective_expiry = min(published_at + ttl_us, local_now + ttl_us)`.

4. **Separate sequence number for descriptor ordering.**
   - Like IPNS, use a monotonic `seq: u64` that the publisher increments on each update. Nodes always prefer higher `seq`. This avoids using wall-clock timestamps for ordering, which is fragile under skew.
   - When `seq` is equal, prefer the descriptor with the later `published_at` (tiebreaker only).

5. **Do not require NTP, but recommend it.**
   - The protocol should function correctly even with 2-minute clock skew between peers. Nodes with wildly wrong clocks will have their descriptors rejected by peers, which is self-correcting (the publisher's descriptors won't propagate).

### Constants

```
MAX_FUTURE_SKEW_US  = 120_000_000   // 120 seconds, in microseconds
MIN_TTL_SECONDS     =          60   // 1 minute minimum TTL
MAX_TTL_SECONDS     =     172_800   // 48 hours maximum TTL (matches IPFS DHT ceiling)
```

### Why Not HLCs?

Hybrid Logical Clocks are excellent for databases where nodes transact with each other and need causal ordering of operations. The Mesh protocol is different: descriptors are **published unilaterally** by capability providers and **validated independently** by each receiving node. There is no causal chain between descriptors from different publishers. The simpler sequence-number + expiry-timestamp pattern provides the needed properties without HLC complexity.

---

## Sources

- BEP-44: Storing Arbitrary Data in the DHT (bittorrent.org/beps/bep_0044.html)
- IPNS Record and Protocol Spec (specs.ipfs.tech/ipns/ipns-record/)
- libp2p Kademlia DHT Specification (github.com/libp2p/specs/kad-dht)
- CockroachDB Transaction Layer (cockroachlabs.com/docs/stable/architecture/transaction-layer.html)
- CockroachDB: Living Without Atomic Clocks (cockroachlabs.com/blog/living-without-atomic-clocks/)
- Bitcoin Block Timestamp (en.bitcoin.it/wiki/Block_timestamp)
- HLC Paper: Kulkarni et al., "Logical Physical Clocks and Consistent Snapshots in Globally Distributed Databases" (Buffalo TR 2014-04)
