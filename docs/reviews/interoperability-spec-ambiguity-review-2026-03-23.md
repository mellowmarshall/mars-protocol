# Interoperability & Spec Ambiguity Review

Scope reviewed: `PROTOCOL.md` and all Rust source under `mesh-core`, `mesh-dht`, `mesh-transport`, and `mesh-node`.

## Critical

### 1. Descriptor ID canonicalization uses a bespoke map-key sort that is not an RFC 8949 deterministic profile
- **Ambiguity or risk:** Section 2.1 says descriptor IDs use "canonical CBOR (deterministic map key ordering)," but Appendix C.1/C.2 then defines raw string-lexicographic key order (`payload`, `publisher`, `routing_keys`, `schema_hash`, `sequence`, `timestamp`, `topic`, `ttl`). That order is neither RFC 8949 Section 4.2.1 nor Section 4.2.3 deterministic CBOR. An implementor who uses an off-the-shelf "deterministic CBOR" mode will hash different bytes and compute different descriptor IDs.
- **Severity:** critical
- **Evidence:** `PROTOCOL.md` Section 2.1 (lines 151-156); Appendix C.1-C.3 (lines 1202-1257); `mesh-core/src/descriptor.rs:69-72,99-159`, which builds the hash input with a `BTreeMap` keyed by Rust strings and then serializes that order.
- **Recommended resolution with specific wording for the spec:**

```text
For descriptor ID computation, implementations MUST use RFC 8949 deterministic encoding, Section 4.2.1. The descriptor-hash input is a CBOR map whose keys MUST appear in this exact order: "ttl", "topic", "payload", "sequence", "publisher", "timestamp", "schema_hash", "routing_keys". Implementations MUST NOT derive key order from native map iteration. All Appendix C test vectors MUST be regenerated to match this rule.
```

### 2. The routing-key prefix is inconsistent inside the spec and in the reference test vector
- **Ambiguity or risk:** Section 4.4 and every mainline example define routing keys as `BLAKE3("mesh:route:" || capability_type)`, but Appendix C.3 says the routing key is computed from `BLAKE3("mesh:routing:...")`. The Rust test vector in `mesh-core/src/descriptor.rs` also uses `mesh:routing:`. These produce different routing keys and therefore different descriptor IDs.
- **Severity:** critical
- **Evidence:** `PROTOCOL.md` Section 4.4/4.5 and examples (lines 413-418, 435-437, 861-863, 879); `PROTOCOL.md` Appendix C.3 (line 1246); `mesh-core/src/routing.rs:10-13,60-64`; `mesh-core/src/descriptor.rs:734-742`.
- **Recommended resolution with specific wording for the spec:**

```text
The routing-key input string is exactly UTF-8("mesh:route:" || capability_type_string). The literal prefix is "mesh:route:" and MUST be used everywhere in this specification, in all examples, and in all test vectors. The string "mesh:routing:" is invalid.
```

### 3. Version `0x01` wire CBOR is not normatively fixed, but the Rust implementation requires one concrete shape
- **Ambiguity or risk:** Appendix C.1 says the canonical form is "not the wire format" and that network serialization "may use any valid CBOR encoding, including arrays or different key orderings." The reference implementation does not implement that freedom: it serializes named Rust structs with Serde/Ciborium, omits absent optionals with `skip_serializing_if`, and expects the same structure back. Another implementation that uses arrays, `null`, tagged values, or a different field layout may still be "valid CBOR" per the spec text but will not be byte-identical and may fail to decode against the Rust implementation.
- **Severity:** critical
- **Evidence:** `PROTOCOL.md` Appendix C.1 (lines 1207-1209); message/body sections 3.4-3.7 (lines 250-360); `mesh-core/src/message.rs:31-41,47-132,135-145`; `mesh-core/src/hash.rs:15-20`; `mesh-core/src/identity.rs:22-27`; `mesh-core/src/descriptor.rs:31-59`.
- **Recommended resolution with specific wording for the spec:**

```text
All Section 3 message bodies and all nested protocol structures in version 0x01 MUST use a single wire representation: CBOR maps with text-string keys exactly matching the field names in this specification. These maps MUST be deterministically encoded using RFC 8949 Section 4.2.1. Optional fields MUST be omitted when absent; senders MUST NOT encode absence as null. Duplicate keys and indefinite-length items are invalid and MUST be rejected.
```

### 4. The request-message bodies in the spec do not match the request-message bodies in the reference implementation
- **Ambiguity or risk:** The Rust implementation requires `sender_addr` in `STORE`, `FIND_NODE`, and `FIND_VALUE`, but the spec only includes it in `PING`. A conforming implementation built from the current text will emit request bodies that the reference implementation does not expect.
- **Severity:** critical
- **Evidence:** `PROTOCOL.md` Section 3.5 (`STORE`, lines 274-279), Section 3.6 (`FIND_NODE`, lines 300-305), Section 3.7 (`FIND_VALUE`, lines 326-333); `mesh-core/src/message.rs:56-64,67-75,78-91`; `mesh-dht/src/node.rs:111-177,320-329`.
- **Recommended resolution with specific wording for the spec:**

```text
STORE (0x02) body:
{
  "sender": Identity,
  "sender_addr": NodeAddr,
  "descriptor": Descriptor
}

FIND_NODE (0x03) body:
{
  "sender": Identity,
  "sender_addr": NodeAddr,
  "target": Hash
}

FIND_VALUE (0x04) body:
{
  "sender": Identity,
  "sender_addr": NodeAddr,
  "key": Hash,
  "max_results": u16,
  "filters": FilterSet?
}
```

### 5. The message-authentication model is internally contradictory
- **Ambiguity or risk:** The design philosophy says "Every message is signed" and Section 1.4 says "Every signed structure in the protocol uses `Signed<T>`," but Section 3 defines plain frame bodies without signatures and the Rust implementation sends unsigned `PING`, `STORE`, `FIND_NODE`, and `FIND_VALUE` messages. An implementor following Section 1.4 literally could wrap or sign request bodies and become incompatible with the current wire format.
- **Severity:** critical
- **Evidence:** `PROTOCOL.md` Design Philosophy item 4 (line 18); Section 1.4 (lines 95-109); Section 3.2-3.7 (lines 214-360); `mesh-core/src/message.rs:47-132`; `mesh-core/src/frame.rs:32-132`.
- **Recommended resolution with specific wording for the spec:**

```text
In version 0x01, Section 3 wire messages are not wrapped in Signed<T> and are not independently signed. Live message authenticity is provided by QUIC/TLS identity binding plus the explicit sender fields in the message body. The Signed<T> envelope is only used when a schema explicitly embeds such an object. Design Philosophy item 4 is therefore: "Every stored descriptor is signed."
```

### 6. Core payload schemas use floating-point values without deterministic encoding rules
- **Ambiguity or risk:** Section 1 says structured data uses CBOR, and descriptor IDs include raw `payload` bytes. The core `capability` and `discovery-query` schemas use `float` in several fields. CBOR permits multiple encodings for the same numeric value, and language runtimes differ in how they produce half/single/double precision. Two implementations can describe the same capability but publish different descriptor IDs.
- **Severity:** critical
- **Evidence:** `PROTOCOL.md` Conventions (line 32); capability schema floats (lines 496-497, 515, 552-553); example publish flow (line 850); the reference CLI never exercises canonical CBOR here because it emits JSON bytes instead (`mesh-node/src/main.rs:282-288`).
- **Recommended resolution with specific wording for the spec:**

```text
Core schemas used inside Descriptor.payload MUST NOT use CBOR floating-point numbers. Replace each float field with a deterministic fixed-point or string form:
- geo.center = [lat_e7: int, lon_e7: int]
- radius_m = uint
- current_load_milli = uint  ; 0..1000
- min_capacity_milli = uint  ; 0..1000
If a future schema requires CBOR floats, it MUST state the exact IEEE 754 width and encoding rules for every float field.
```

## High

### 7. DID derivation disagrees between the spec and the reference implementation
- **Ambiguity or risk:** The spec defines `did:mesh:<multibase-base58btc(...)>`, which implies a multibase `z` prefix. The Rust implementation emits bare base58 bytes with no multibase prefix. Different implementations will derive different DID strings from the same public key.
- **Severity:** high
- **Evidence:** `PROTOCOL.md` Section 1.3 (lines 84-93); `mesh-core/src/identity.rs:39-45`; `mesh-transport/src/tls.rs:124-138`, which also uses the derived DID in the certificate subject.
- **Recommended resolution with specific wording for the spec:**

```text
The DID string for an identity is:
did:mesh:z<base58btc(algorithm_byte || public_key_bytes)>
The leading multibase prefix "z" is mandatory. Implementations MUST use lowercase "did:mesh:" and MUST reject method-specific identifiers that omit the "z" prefix.
```

### 8. Several schema fields are typed as raw `bstr` even though they semantically carry `Hash`, `Identity`, or DID values
- **Ambiguity or risk:** `descriptor_id`, `target_id`, and `successor` are written as `bstr`; `old_identity` and `new_identity` are also `bstr`; `requester` is `bstr` but the comment says "requester's DID." This leaves multiple incompatible interpretations open: raw digest bytes, encoded `Hash` structs, encoded `Identity` structs, or UTF-8 DID bytes.
- **Severity:** high
- **Evidence:** `PROTOCOL.md` resolve schema (lines 569-582), revocation schema (lines 592-597), key-rotation schema (lines 654-660), and the base `Hash`/`Identity` definitions in Sections 1.2-1.3 (lines 61-81).
- **Recommended resolution with specific wording for the spec:**

```text
The following schema fields carry protocol primitives and MUST use those primitives directly, not raw bstr:
- descriptor_id, target_id, successor, updated: Hash
- old_identity, new_identity: Identity
If requester is a DID, its type is tstr and the field definition is:
  requester: tstr  ; did:mesh DID string
Raw bstr is reserved for fields that explicitly carry uninterpreted bytes and state an exact byte length.
```

### 9. The reference CLI publishes `core/capability` payloads as JSON, not CBOR
- **Ambiguity or risk:** Section 5.2 defines a CBOR/CDDL payload, and Section 11.1 says "Serialize payload as CBOR." The `mesh-node` publisher instead builds JSON and stores `serde_json::to_vec(...)` bytes in `Descriptor.payload`. An implementor using the CLI as a reference will produce payload bytes that are incompatible with the schema text and with any consumer expecting CBOR.
- **Severity:** high
- **Evidence:** `PROTOCOL.md` Section 5.2 (lines 476-532), Section 11.1 step 2 (line 850); `mesh-node/src/main.rs:282-288`; `mesh-node/src/main.rs:393-395`, which also tries to decode discovered payloads as JSON.
- **Recommended resolution with specific wording for the spec:**

```text
For schema core/capability, Descriptor.payload MUST contain the deterministic CBOR encoding of the Section 5.2 CDDL object. JSON is not an equivalent representation in version 0x01 and MUST NOT be published under schema_hash = BLAKE3("mesh:schema:core/capability").
```

### 10. The spec does not provide enough normative test vectors to validate another implementation
- **Ambiguity or risk:** Appendix B gives placeholders instead of actual schema-hash bytes, and Appendix C only covers one descriptor-hash vector. There is no normative DID vector, no routing-key vector, no full wire-frame vector, and no example of the exact CBOR bytes for a real request/response body. That makes it difficult to detect mismatches like the `route`/`routing` bug before implementations ship.
- **Severity:** high
- **Evidence:** `PROTOCOL.md` Appendix B (lines 1185-1196); Appendix C (lines 1198-1273); the Rust code has a descriptor vector (`mesh-core/src/descriptor.rs:723-837`) but no DID or frame conformance vector.
- **Recommended resolution with specific wording for the spec:**

```text
Appendix B and Appendix C are normative. They MUST include:
1. One Ed25519 secret key -> public key -> DID vector
2. One schema name -> schema hash vector
3. One capability type -> routing key vector
4. One descriptor input -> canonical descriptor-hash CBOR bytes -> descriptor ID vector
5. One STORE request -> exact frame bytes vector
6. One FIND_VALUE response -> exact frame bytes vector
Each vector MUST include the exact input bytes, exact serialized bytes, and exact expected hex output.
```

### 11. Equal-sequence updates are underspecified for non-identical descriptors
- **Ambiguity or risk:** Section 2.2 only says to ignore lower sequence numbers. The reference implementation accepts equal-sequence descriptors and silently replaces the stored version. Another implementation could reject equal sequence, keep both, or compare timestamps. That can cause different nodes to converge on different "current" descriptors for the same `(publisher, schema_hash, topic)` slot.
- **Severity:** high
- **Evidence:** `PROTOCOL.md` Section 2.2 step 8 and Section 2.3 (lines 181-197); `mesh-dht/src/storage.rs:136-165`; `mesh-dht/src/storage.rs:649-660`.
- **Recommended resolution with specific wording for the spec:**

```text
For a fixed (publisher, schema_hash, topic) tuple:
- if incoming.sequence < stored.sequence: reject as stale
- if incoming.sequence == stored.sequence and incoming.id == stored.id: accept as idempotent replay
- if incoming.sequence == stored.sequence and incoming.id != stored.id: reject as conflicting update
- if incoming.sequence > stored.sequence: replace the stored descriptor
```

## Medium

### 12. Appendix A assigns CBOR tags but never says whether version `0x01` uses them
- **Ambiguity or risk:** Tags 42-45 are defined for `Identity`, `Hash`, `Descriptor`, and `Frame`, but the spec never says whether senders must emit them, may emit them, or must not emit them. The Rust implementation does not emit or consume any of these tags. A tag-using implementation would therefore change the bytes on the wire without a shared rule.
- **Severity:** medium
- **Evidence:** `PROTOCOL.md` Appendix A (lines 1174-1183); no corresponding tag-handling code appears in `mesh-core/src/hash.rs`, `mesh-core/src/identity.rs`, `mesh-core/src/descriptor.rs`, `mesh-core/src/message.rs`, or `mesh-core/src/frame.rs`.
- **Recommended resolution with specific wording for the spec:**

```text
CBOR tags 42-45 are reserved for future profiles and are not used in version 0x01. Senders MUST NOT emit these tags in wire messages or in descriptor-hash inputs unless a future version explicitly enables them.
```

### 13. `FindValueResult`'s one-of rule is stated informally but not made a receiver requirement
- **Ambiguity or risk:** Section 3.7 says "Exactly one of these will be populated," but it does not define the invalid cases precisely. The reference implementation serializes the absent side by omitting the field, but nothing in the spec tells receivers what to do if both keys are present, if both are absent, or if an absent field is encoded as `null`.
- **Severity:** medium
- **Evidence:** `PROTOCOL.md` Section 3.7 (lines 336-347); `mesh-core/src/message.rs:123-132`; `mesh-dht/src/node.rs:158-177`.
- **Recommended resolution with specific wording for the spec:**

```text
FindValueResult MUST contain exactly one of the keys "descriptors" or "nodes". The absent member MUST be omitted, not encoded as null. A receiver MUST reject a FindValueResult that contains both keys or neither key.
```

### 14. Section 3.2 does not fully pin down the CBOR payload framing details
- **Ambiguity or risk:** The frame header is precise, but the body rules stop at "CBOR-encoded message body." The spec never says whether `body` contains exactly one CBOR item, whether the self-described CBOR tag `0xd9d9f7` is allowed, or whether duplicate keys/trailing bytes are valid. Those choices affect cross-language decoders and any future byte-for-byte test vectors.
- **Severity:** medium
- **Evidence:** `PROTOCOL.md` Section 3.2 (lines 216-225); `mesh-core/src/frame.rs:78-132`; `mesh-core/src/message.rs:143-145`.
- **Recommended resolution with specific wording for the spec:**

```text
Frame.body contains exactly one CBOR data item and no trailing bytes. Senders MUST NOT prepend the CBOR self-described tag (0xd9d9f7). Receivers MUST reject duplicate map keys, indefinite-length items, and trailing bytes after the first complete CBOR item.
```
