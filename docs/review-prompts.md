# Review Prompts for Capability Mesh Protocol

Hand each of these to a separate agent. Each prompt is self-contained.

---

## 1. Senior Software Engineer Review

You are a senior software engineer reviewing a distributed systems project. The project is a Kademlia-based DHT protocol for decentralized capability discovery — agents publish signed capability descriptors and other agents discover them via content-addressed routing keys over QUIC.

**Your files to review:**
- `PROTOCOL.md` — the wire specification (this is the primary artifact)
- `PLAN-01-mesh-hub.md` — design doc for a hub node (high-capacity infrastructure node)
- All Rust source code in the workspace

**Review for:**

1. **Correctness of the protocol design.** Look for race conditions, edge cases, and logical inconsistencies. Specifically:
   - Can the descriptor republishing mechanism (Section 2.3) lose data? What happens if a publisher crashes mid-republish — old descriptor expired, new one not yet stored?
   - Is the deduplication key (`publisher + schema_hash + topic`) sufficient? Can two different descriptors collide on this key unintentionally?
   - Can the sequence number mechanism be gamed? What if an attacker replays a high sequence number to permanently shadow a publisher's real descriptors?
   - Are there ordering issues with revocations? What if a revocation arrives before the descriptor it revokes?

2. **Implementation correctness.** Review all Rust code for:
   - Serialization correctness — CBOR canonical form, deterministic key ordering, byte-level reproducibility. This is the highest-priority concern. If two implementations produce different bytes for the same logical data, content addressing breaks and the entire protocol fails. Look for: implicit field ordering, floating point in serialized data, non-deterministic map iteration, platform-dependent integer sizes.
   - Hash computation — are all inputs to BLAKE3 unambiguous? Could two different logical structures produce the same serialized bytes (collision by construction, not hash collision)?
   - Signature verification — is the sign-then-hash vs hash-then-sign ordering consistent everywhere?
   - Error handling — are there panics in protocol-critical paths? Are malformed messages handled gracefully without crashing the node?

3. **API design and developer experience.** If someone wanted to implement a mesh node in Go or Python using only PROTOCOL.md as their guide:
   - Are there ambiguities that would lead to incompatible implementations?
   - Are all byte formats specified precisely enough?
   - Are there implicit assumptions that only make sense in Rust?

4. **Scalability concerns.** With the current design:
   - What breaks first at 10K nodes? 100K nodes? 1M nodes?
   - Are the DHT parameters (K=20, α=3) appropriate? What's the lookup latency at different network sizes?
   - The hub's hot path (in-memory tenant cache → token bucket → atomic MU decrement) — are there contention points under high concurrency?

5. **Dependency risk.** Evaluate every external dependency:
   - Is it maintained? Bus factor?
   - Does it have correctness guarantees needed for a protocol implementation (e.g., CBOR library must produce canonical output)?
   - Are there alternatives if a dependency dies?

**Output format:** For each finding, state: (a) what you found, (b) severity (critical / high / medium / low), (c) your recommended fix, (d) which file and section it applies to. Group by severity, criticals first.

---

## 2. Security & Hardening Review

You are a security engineer reviewing a decentralized protocol and its reference implementation. The system is a peer-to-peer capability discovery mesh using Kademlia DHT over QUIC. All data is signed with Ed25519. Identities are self-certifying DIDs derived from public keys. There is no central authority.

**Your files to review:**
- `PROTOCOL.md` — the wire specification, especially Sections 9 (Security Considerations), 7 (Identity Management), and 1.4 (Signature Envelope)
- `PLAN-01-mesh-hub.md` — hub node design, especially Sections 9 (Security) and 5 (Multi-Tenant)
- `docs/private/hub-payment-implementation.md` — payment and entitlement system
- All Rust source code in the workspace

**Review for:**

1. **Cryptographic soundness.**
   - Is Ed25519 used correctly? Are there nonce reuse risks, malleability issues, or small-subgroup attacks?
   - Is the signature envelope (Section 1.4) secure? Specifically: signing `payload` bytes directly vs signing a hash of the payload — are there length-extension or substitution attacks?
   - Algorithm agility (Section 1.1) — does supporting multiple algorithms create downgrade attack vectors? Can an attacker force a node to accept a weaker algorithm?
   - BLAKE3 as content-addressing hash — are there second-preimage concerns for the descriptor ID scheme? Can an attacker craft two different descriptors with the same ID?
   - The DID derivation (`did:mesh:<multibase(algorithm_byte || public_key)>`) — are there canonicalization issues that could make two different DID strings resolve to the same key, or the same DID string resolve to different keys?

2. **Protocol-level attacks.**
   - **Sybil attacks** — the spec acknowledges these (Section 9.1). Are the mitigations sufficient? Can an attacker with 1000 keypairs meaningfully disrupt the DHT?
   - **Eclipse attacks** — can an attacker isolate a node by filling its routing table with colluding nodes? How many nodes would this require given K=20?
   - **Routing table poisoning** — can an attacker inject false NodeInfo entries to misdirect lookups?
   - **Descriptor poisoning** — can an attacker publish valid-looking but misleading descriptors (e.g., advertising a capability they don't actually provide) to waste consumers' time?
   - **Replay attacks** — can old, valid descriptors be replayed? The sequence number mechanism should prevent this, but verify.
   - **Timestamp manipulation** — the spec adds a 120-second future guard. Is this sufficient? Can an attacker exploit the `min(timestamp, now) + ttl` expiry computation?
   - **Resource exhaustion** — can an attacker exhaust a node's storage, memory, or CPU through protocol-compliant behavior? What's the cost to the attacker vs the cost to the victim?

3. **Hub-specific attack surface.**
   - Admin API — even bound to localhost, is there SSRF risk? Can a malicious tenant craft requests that reach admin endpoints?
   - Tenant isolation — can tenant A observe tenant B's descriptors, usage patterns, or identities through timing attacks, storage pressure, or rate limit probing?
   - DID-auth challenge-response — is the challenge generation secure? Are challenges replayable across hubs?
   - The hot path (in-memory tenant cache) — can an attacker cause cache thrashing by rotating identities rapidly?
   - Payment system — can an agent consume MUs without paying (race conditions between metering and enforcement)? Can a trial account be recreated indefinitely with fresh DIDs?

4. **Privacy analysis.**
   - What can a passive observer (a node participating honestly in the DHT) learn about other participants?
   - What can an active observer (a node logging all queries it receives) learn?
   - Can a hub operator correlate tenant identities with query patterns to profile behavior?
   - Is there metadata leakage in QUIC connection patterns (connection timing, frequency, peer selection)?

5. **Supply chain and operational security.**
   - Review all dependencies for known vulnerabilities.
   - Are cryptographic keys stored securely? What happens if a hub's keypair file is readable by other processes?
   - Are there secrets (API keys, Stripe tokens) that could leak through logs, error messages, or metrics?

**Output format:** For each finding, state: (a) the attack or vulnerability, (b) severity (critical / high / medium / low), (c) exploitability (trivial / moderate / difficult / theoretical), (d) your recommended mitigation, (e) which file and section it applies to. Group by severity, criticals first.

---

## 3. Interoperability & Spec Ambiguity Review

You are a protocol engineer reviewing a specification for cross-implementation compatibility. The protocol will have implementations in multiple languages (Rust, Go, Python, TypeScript at minimum). The SINGLE most important property is that all implementations produce byte-identical output for the same logical input — content addressing breaks if they don't.

**Your files to review:**
- `PROTOCOL.md` — the wire specification (every section)
- All Rust source code in the workspace (as the reference implementation)

**Review for:**

1. **Serialization determinism.** This is the top priority. For every structure that gets serialized to bytes (descriptors, frames, signatures, hashes):
   - Is the CBOR encoding fully specified? RFC 8949 defines "deterministically encoded CBOR" but implementations vary. Which specific rules apply? (sorted keys? minimal integer encoding? no indefinite-length? no duplicate keys?)
   - Are map keys always strings? If mixed types are possible, what's the sort order?
   - Are there any optional fields? How are they represented — omitted key, or key with null value? Different choices produce different bytes.
   - Are there any floating-point values? IEEE 754 has multiple valid encodings for the same number in CBOR. If floats exist, specify the encoding.
   - What happens with empty arrays, empty maps, empty strings, zero-value integers? Are these all unambiguous?

2. **Hash input ambiguity.** For every hash computation in the spec:
   - `descriptor.id = BLAKE3(canonical CBOR of fields)` — which fields, in what order? The spec says deterministic map key ordering, but spell out the exact CBOR map key order for each structure.
   - `routing_key = BLAKE3("mesh:route:" || capability_type)` — is the `||` byte concatenation? Is capability_type UTF-8? Is there a length prefix or delimiter? Could `mesh:route:a/b` and `mesh:route:a` with `/b` appended collide?
   - `node_id = BLAKE3(public_key_bytes)` — raw bytes, no prefix? Confirm.
   - Schema hashes `BLAKE3("mesh:schema:core/capability")` — same questions.

3. **Wire format precision.** For the frame format (Section 3.2):
   - Magic bytes, version, msg_type, msg_id, body_len — are these all in network byte order (big-endian)? The conventions say big-endian, but confirm it applies to frame headers too.
   - Is msg_id random bytes or a UUID? If UUID, which version? If random, what's the entropy source requirement?
   - Body is CBOR — does the body include CBOR self-describing tag (0xd9d9f7)? Probably not, but specify.

4. **Type ambiguity across languages.**
   - `u64` timestamp in microseconds — what happens in JavaScript where all numbers are IEEE 754 doubles? Max safe integer is 2^53 - 1. Microsecond timestamps for year 2026 are ~1.7×10^15, which is within range, but will it always be?
   - `bytes` fields — are these CBOR byte strings (major type 2) or text strings (major type 3)? Be explicit for each field.
   - `Identity` and `Hash` types — when these appear as CBOR map values, are they encoded as byte strings containing the raw struct, or as nested CBOR maps with named fields?
   - The `Signed<T>` envelope — `payload` is "canonical CBOR serialization of T" stored as a byte string. Confirm: this is CBOR-in-CBOR (the payload field is a CBOR byte string whose contents are themselves a CBOR structure)?

5. **Test vectors.** Does the spec include test vectors? It should. At minimum:
   - A known Ed25519 keypair → expected DID string
   - A known schema name → expected schema hash (full 32 bytes, hex)
   - A known capability type → expected routing key
   - A known descriptor (all fields specified) → expected descriptor ID
   - A known descriptor → expected serialized bytes (hex dump)
   - A known frame → expected wire bytes

   Without test vectors, implementors in other languages have no way to verify compatibility except by testing against the Rust implementation directly.

6. **Underspecified behavior.** Look for areas where the spec says "implementation-defined" or is silent, and where different choices would break interop:
   - What CBOR tags are used and when? Section Appendix A defines tags 42-45 but doesn't say when they're required vs optional.
   - When the spec says "canonical CBOR (deterministic map key ordering)" — specify: RFC 8949 Section 4.2.1 (length-first key sorting) or RFC 8949 Section 4.2.3 (lexicographic sorting of encoded keys)?

**Output format:** For each finding, state: (a) the ambiguity or risk, (b) severity (critical = will cause interop failure / high = likely to cause interop failure / medium = may cause confusion / low = editorial), (c) your recommended resolution with specific wording for the spec. Group by severity, criticals first.

---

## 4. Cryptography Review

You are a cryptographer reviewing the cryptographic design of a decentralized protocol. The protocol uses Ed25519 for signatures, BLAKE3 for hashing, X25519 for key exchange, and has reserved slots for post-quantum algorithms (ML-DSA-65, ML-KEM-768). Identities are self-certifying — derived directly from public keys with no PKI or certificate authority.

**Your files to review:**
- `PROTOCOL.md` — Sections 1.1 (Algorithm Registry), 1.2 (Hash Format), 1.3 (Identity Format), 1.4 (Signature Envelope), 2.1 (Content Addressing), 7 (Identity Management)
- All Rust source code that performs cryptographic operations

**Review for:**

1. **Algorithm choices.**
   - Ed25519 for signatures — appropriate for this use case? Are there cofactor / malleability concerns that matter for a descriptor-signing protocol? Should the spec mandate cofactored verification (RFC 8032) or is the more common cofactorless verification acceptable?
   - BLAKE3 for content addressing — appropriate? Are there length-extension concerns? (BLAKE3 is not vulnerable, but verify the usage doesn't inadvertently create one.) Is 32-byte output sufficient for collision resistance at the expected scale?
   - X25519 for key exchange — the spec reserves it but doesn't define where it's used. Is there a missing key exchange step (e.g., for the resolve/negotiate phase)?
   - Single-byte algorithm IDs (0x00-0xFF) — is 256 slots sufficient for algorithm agility over the protocol's intended lifespan?

2. **Signature scheme analysis.**
   - The signature envelope signs `payload` bytes directly (not a hash of the payload). Is this correct for Ed25519, which internally hashes? Are there any size or performance implications?
   - Actually, descriptors sign `id.digest` (the BLAKE3 hash), not the payload directly. So the chain is: CBOR → BLAKE3 → Ed25519.sign(hash). Is hash-then-sign safe here? Are there any interaction issues between BLAKE3 and Ed25519's internal SHA-512?
   - Key rotation (Section 7.1) requires the OLD key to sign the NEW identity, and includes an `old_signature` field that signs the new identity with the old key. Is this double-proof scheme sufficient to prevent key rotation attacks? Can an attacker who compromises the old key after rotation use it to perform a second rotation to a key they control?
   - Ephemeral identities (Section 7.3) — are there risks if an agent generates thousands of keypairs? Key generation quality, entropy exhaustion?

3. **Content addressing security.**
   - The descriptor ID is BLAKE3 of the canonical CBOR serialization. An attacker cannot forge a descriptor with a specific ID (preimage resistance), but can they craft two descriptors with the same ID (collision)? At what cost?
   - The routing key is BLAKE3 of a well-known string prefix + capability type. An attacker CAN predict routing keys (they're deterministic). Is this a problem? Can they precompute node IDs that land in specific k-buckets near high-value routing keys?
   - Schema hashes are BLAKE3 of well-known strings. Same question — predictable hashes. Any attack vector?

4. **Post-quantum readiness.**
   - ML-DSA-65 signatures are 3309 bytes. The current frame header uses u32 for body_len, which is fine. But does the 65KB payload limit (Section 2.2) accommodate ML-DSA signatures? A descriptor with an ML-DSA signature + ML-DSA public key + payload could approach the limit.
   - Hybrid signatures (Ed25519 + ML-DSA) — should the spec support signing with both a classical and post-quantum algorithm for transition security? This isn't defined currently.
   - What's the migration path? When ML-DSA is activated, do old Ed25519-signed descriptors become untrusted? Or do they coexist?

5. **Implementation review.** In the Rust code:
   - Which Ed25519 library is used? Does it implement RFC 8032 correctly? Is it a well-audited crate (ed25519-dalek, ring)?
   - Is the BLAKE3 crate the official `blake3` crate?
   - Are there any cases where cryptographic keys are logged, serialized to debug strings, or otherwise exposed?
   - Is key material zeroized on drop? (Use `zeroize` crate.)
   - Are random values (msg_id, challenge nonces) generated from a CSPRNG?

**Output format:** For each finding, state: (a) the concern, (b) severity (critical / high / medium / low), (c) whether this is a spec issue or implementation issue, (d) your recommended fix. Group by severity, criticals first.
