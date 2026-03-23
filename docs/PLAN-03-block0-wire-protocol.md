# PLAN-03: Block 0 — Wire Protocol Stabilization

**Status:** implemented
**Prerequisite for:** Phase 1 Hub Peering, all subsequent hub phases
**Decision:** Hybrid auth model — TLS identity binding for transport, `Signed<T>` (Ed25519) for descriptors only

---

## Task 1: Auth Model Normalization — status: implemented

### 1a: Update PROTOCOL.md auth model — spec — status: implemented
- Rewrote Design Principle 4 to describe hybrid model
- Replaced §1.4 with "Authentication Model" section distinguishing descriptor signatures from TLS binding
- Moved `Signed<T>` to §1.4.1 (Reserved) for forward compatibility
- Strengthened §3.1.1 with four numbered normative requirements (mutual TLS, cert derivation, peer extraction, sender verification)

### 1b: Implement TLS certificate validation — code — status: implemented
- `mesh-transport/src/tls.rs`: Added `MeshServerCertVerifier` with Ed25519 key extraction validation
- Added `MeshClientCertVerifier` implementing `ClientCertVerifier` (enables mutual TLS)
- Updated `server_crypto_config` to use `with_client_cert_verifier`
- Updated `client_crypto_config` to accept cert chain + key for client auth

### 1c: Add sender-TLS binding check — code — status: implemented
- Added `verify_sender_binding()` in `mesh-dht/src/node.rs` (re-exported from `mesh-dht`)
- Checks `msg.sender == peer_identity`, rejects on mismatch, warns on missing peer identity
- `mesh-node/src/main.rs` and `mesh-hub/src/lib.rs` refactored to use shared function

---

## Task 2: CBOR Canonicalization Fix — status: implemented

### 2a: Fix descriptor.rs canonical_cbor_bytes — code — status: implemented
- Replaced `BTreeMap<&str, Value>` with `Value::Map(Vec<(Value, Value)>)` in RFC 8949 §4.2.1 order
- Canonical field order: `ttl, topic, payload, sequence, publisher, timestamp, schema_hash, routing_keys`
- All test hashes regenerated, 156 tests passing

### 2b: Update PROTOCOL.md canonicalization rule — spec — status: implemented
- Updated Appendix C.1: normative reference to RFC 8949 §4.2.1
- Updated Appendix C.2: field table reordered with CBOR key lengths and sort explanation
- Updated Appendix C.3: new canonical CBOR hex and BLAKE3 hash test vectors

### 2c: Regenerate test vectors — code+spec — status: implemented
- New test vectors generated from fixed code, PROTOCOL.md updated

---

## Task 3: Spec-Implementation Alignment — status: implemented

### 3a: Add sender_addr to all message definitions — spec — status: already-done
- All four request messages already had `sender` and `sender_addr` in both spec and code

### 3b: Float → integer in schema documentation — spec — status: already-done
- Schemas already used integer types: microdegrees, meters, permille, decimal strings
- No float fields found in spec or code

### 3c: Verify routing key prefix consistency — spec+code — status: already-done
- All occurrences use `mesh:route:` consistently

---

## Task 4: Architecture Decision Record — status: implemented

### 4a: Create docs/architecture-context.md — status: implemented
- ADR-001: Hybrid authentication model (rationale, implications)
- ADR-002: RFC 8949 §4.2.1 deterministic CBOR (rationale, canonical field order)
- ADR-003: Integer-only schemas (rationale)

---

## Verification

- 156 tests passing (0 failures) across 6 crates
- `cargo clippy` clean on modified code
