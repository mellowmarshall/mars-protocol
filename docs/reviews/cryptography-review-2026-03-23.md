# Cryptography Review

Scope reviewed:
- `PROTOCOL.md` with emphasis on Sections 1.1, 1.2, 1.3, 1.4, 2.1, and 7
- Rust code that performs cryptographic operations, primarily in `mesh-core` and `mesh-transport`

Validation performed:
- `cargo test -p mesh-core descriptor -- --nocapture`
- `cargo test -p mesh-transport tls -- --nocapture`

Overall assessment:
- The core primitive choices are reasonable for a v1 design: `ed25519-dalek` and the official `blake3` crate are in use (`Cargo.toml:22-26`, `mesh-core/Cargo.toml:6-15`).
- The main problems are not the raw primitives themselves. The highest-risk issues are authentication-model contradictions, disabled transport identity verification in the implementation, DID canonicalization drift, and key-rotation semantics that do not survive compromise cleanly.

## Critical

### Finding 1
- Concern: The reference implementation does not actually authenticate peers or control-plane senders. The client-side TLS verifier accepts any certificate and any TLS handshake signature without checking certificate structure, certificate/public-key algorithm, self-signature, or binding to the mesh `sender` identity. On the server side, client certificate authentication is disabled entirely. The DHT then trusts unauthenticated `sender` fields from request bodies and inserts them into the routing table.
- Severity: critical
- Spec or implementation issue: implementation issue
- Recommended fix: Enforce a real mutual-authentication story. At minimum: require client certificates, validate that the peer certificate is an Ed25519 certificate carrying the expected mesh identity, parse the certificate with a proper X.509/SPKI parser instead of byte-pattern scanning, and reject any request whose application-level `sender` does not equal the authenticated TLS identity. If the protocol intends message-level signatures instead of mutual TLS, implement `Signed<T>` verification on every control-plane message and do not trust bare `sender` fields.
- Evidence:
  - `mesh-transport/src/tls.rs:30-75` returns success unconditionally from `verify_server_cert`, `verify_tls12_signature`, and `verify_tls13_signature`, and advertises RSA/ECDSA schemes in addition to Ed25519.
  - `mesh-transport/src/tls.rs:156-173` uses `with_no_client_auth()` on both server and client configs.
  - `mesh-transport/src/connection.rs:49-60` exposes `peer_mesh_identity()`, but no caller uses it to authenticate message senders.
  - `mesh-core/src/message.rs:46-132` defines bare request/response structs with `sender` fields but no signatures.
  - `mesh-dht/src/node.rs:95-153` updates routing state directly from those unverified `sender` and `sender_addr` fields.

### Finding 2
- Concern: The specification is internally contradictory about message authentication. Section 0 says “Every message is signed,” and Section 1.4 defines a generic `Signed<T>` envelope, but the actual wire-message definitions in Sections 3.4-3.7 are plain CBOR maps with no signature fields or wrapper. Two independent implementers could both claim conformance while choosing incompatible authentication behavior, and one obvious interpretation is “unsigned control messages.”
- Severity: critical
- Spec or implementation issue: spec issue
- Recommended fix: Pick one wire-level rule and state it normatively. Either: 1. every message body in Sections 3.4-3.7 is wrapped in `Signed<T>` and the receiver MUST verify it before acting on `sender`; or 2. control-plane authentication is bound exclusively to mutual TLS, in which case remove or narrow the “Every message is signed” claim and specify exactly how TLS identities are bound to message fields. The current text cannot be implemented securely without guesswork.
- Evidence:
  - `PROTOCOL.md:14-21` states the design principle “Every message is signed. No unsigned communication.”
  - `PROTOCOL.md:95-109` defines `Signed<T>` as the universal signature envelope.
  - `PROTOCOL.md:250-343` defines `PING`, `PONG`, `STORE`, `FIND_NODE`, and `FIND_VALUE` as unsigned CBOR structures.
  - `mesh-core/src/message.rs:46-132` matches the unsigned interpretation in code.

## High

### Finding 3
- Concern: DID canonicalization is inconsistent between the spec and the implementation. The spec requires `multibase-base58btc(...)`, which implies a multibase prefix such as `z`. The implementation emits plain base58btc with no multibase prefix. That means the same public key can have different textual DIDs across implementations, which is exactly the kind of canonicalization failure self-certifying identity formats must avoid.
- Severity: high
- Spec or implementation issue: implementation issue
- Recommended fix: Change the implementation to emit and accept the exact DID form in the spec, including the multibase prefix. Add a normative test vector in `PROTOCOL.md` for a known keypair -> expected DID string, and add round-trip tests that reject non-canonical spellings.
- Evidence:
  - `PROTOCOL.md:84-93` specifies `did:mesh:<multibase-base58btc(algorithm_byte || public_key_bytes)>`.
  - `mesh-core/src/identity.rs:39-45` formats `did:mesh:` plus raw `bs58` output, with no multibase prefix.
  - `mesh-transport/src/tls.rs:127-139` uses that DID string as the certificate subject CN, so the mismatch propagates into transport identity metadata.

### Finding 4
- Concern: Key rotation is not anchored to a single monotonic chain, so compromise of an old key after a valid rotation can still produce a competing later rotation. The current `key-rotation` object carries only `old_identity`, `new_identity`, `effective`, and an `old_signature` over the new identity. There is no rotation sequence number, no previous-rotation hash, no revocation epoch, and no rule for rejecting stale or forked rotations once an old key has supposedly become inactive.
- Severity: high
- Spec or implementation issue: spec issue
- Recommended fix: Define rotation as a signed transition chain, not as a one-off statement. Add a monotonic rotation counter or previous-rotation hash, require both old and new keys to sign a canonical transition object, and specify the exact state machine nodes must use to reject forks and stale rotations. If compromise recovery is meant to rely on pre-committed recovery keys, make the recovery path part of the normative protocol instead of an optional Layer 3 convention.
- Evidence:
  - `PROTOCOL.md:650-669` defines the rotation object and says nodes “receiving this update their routing tables.”
  - `PROTOCOL.md:673-681` acknowledges post-compromise recovery problems but leaves recovery keys as an optional higher-layer pattern.

## Medium

### Finding 5
- Concern: The algorithm registry is too weak for cryptographic agility. New algorithm IDs are allocated by publishing descriptors to the mesh, and conflicting assignments are resolved by “adoption.” That is not sufficient for a one-byte cryptographic identifier that is embedded directly in identities and hashes. A conflicting assignment to the same byte does not degrade gracefully; it creates incompatible verification semantics for the same on-wire identifier.
- Severity: medium
- Spec or implementation issue: spec issue
- Recommended fix: Reserve mesh-wide cryptographic algorithm IDs through a stable, collision-free process. If governance is intentionally minimized, then make ungoverned extensions local-only by construction and keep globally interoperable cryptographic IDs centrally curated or content-addressed by a longer, collision-resistant identifier rather than a single byte.
- Evidence:
  - `PROTOCOL.md:35-57` defines a one-byte registry, allows user-defined ranges, and resolves collisions by “critical mass.”

### Finding 6
- Concern: Secret-key handling leaves plaintext key material exposed longer than necessary and with weak at-rest controls. The CLI writes raw 32-byte private keys directly to disk with default filesystem permissions and can print them to stdout as hex. The TLS layer also materializes PKCS#8 copies of the private key in heap-backed `Vec<u8>` buffers and clones them, with no explicit zeroization visible in the repository.
- Severity: medium
- Spec or implementation issue: implementation issue
- Recommended fix: Create key files with restrictive permissions (`0600` on Unix), avoid printing private keys by default, store them in a structured secret format, and wrap transient secret buffers in explicit zeroizing containers. Also prefer APIs that avoid cloning PKCS#8 plaintext where possible.
- Evidence:
  - `mesh-node/src/main.rs:112-128` reads and writes raw 32-byte secret keys directly.
  - `mesh-node/src/main.rs:443-455` prints the secret key hex when `identity --generate` is used without `--path`.
  - `mesh-core/src/identity.rs:121-123` exposes raw secret bytes.
  - `mesh-transport/src/tls.rs:79-91` builds a PKCS#8 `Vec<u8>` from the secret key.
  - `mesh-transport/src/tls.rs:130-146` clones and stores PKCS#8 plaintext during certificate generation.

### Finding 7
- Concern: The routing-key hash domain string is inconsistent inside the repository: the main spec and routing code use `mesh:route:`, but Appendix C and the descriptor test vector use `mesh:routing:`. Because BLAKE3 is deterministic, this changes the derived routing key completely. That is a cryptographic input mismatch, not an editorial nit.
- Severity: medium
- Spec or implementation issue: spec issue
- Recommended fix: Choose one domain label and use it everywhere: Sections 4.4, 11.1, Appendix C, the routing implementation, and every test vector. Then add a single authoritative test vector for a known capability type -> expected routing key hash.
- Evidence:
  - `PROTOCOL.md:413-418` defines routing keys with `mesh:route:`.
  - `PROTOCOL.md:861-863` uses `mesh:route:` again in the publish example.
  - `PROTOCOL.md:1245-1246` says the Appendix C routing key is `BLAKE3("mesh:routing:compute/inference/text-generation")`.
  - `mesh-core/src/routing.rs:10-13` implements `mesh:route:`.
  - `mesh-core/src/descriptor.rs:734-735` hardcodes the Appendix C test vector with `mesh:routing:`.

## Low

### Finding 8
- Concern: The algorithm registry overstates key-exchange and post-quantum readiness. `X25519` is marked “Required,” and ML-DSA / ML-KEM slots are reserved, but the protocol explicitly says there is no protocol-layer encryption and defines no X25519 or ML-KEM transcript, no hybrid-signature format, and no migration rules for when post-quantum algorithms become active.
- Severity: low
- Spec or implementation issue: spec issue
- Recommended fix: Mark X25519 and the PQ algorithms as reserved until there is a concrete transcript, key-confirmation rule, and migration plan. If the intent is future hybrid mode, specify whether descriptors can carry multiple signatures and how verifiers decide trust during transition.
- Evidence:
  - `PROTOCOL.md:41-57` marks `X25519` required and ML-DSA / ML-KEM reserved.
  - `PROTOCOL.md:1007-1014` says there is no encryption at the protocol layer.
  - No Rust code in the reviewed crypto paths implements X25519, ML-DSA, or ML-KEM operations.
