# Security & Hardening Review

Scope reviewed: `PROTOCOL.md`, `docs/PLAN-01-mesh-hub.md`, `docs/private/hub-payment-implementation.md`, and all Rust source in the workspace. I did not read existing review outputs. I ran `cargo test --workspace --quiet`; all tests passed. Dependency review is limited to repository contents and local manifests/code paths; I did not consult an external advisory database.

## Critical

1. (a) Attack or vulnerability: Mesh request messages are effectively unauthenticated, so any peer can spoof another identity in `PING`, `STORE`, `FIND_NODE`, and `FIND_VALUE`. The spec says "Every message is signed" and defines a `Signed<T>` envelope, but the actual wire messages only carry a `sender` field. The reference node then trusts that claimed sender for routing-table updates, and the hub/payment design meters and classifies clients from the frame identity on the hot path. This creates identity spoofing, tenant-impersonation, false-attribution, and query/metering abuse risk.
   (b) Severity: critical
   (c) Exploitability: trivial
   (d) Recommended mitigation: Make every protocol request/response cryptographically authenticated, not just descriptors. The clean fix is to wrap each message body in a signed envelope that covers at least `msg_type`, `msg_id`, the body bytes, and replay-resistant freshness data. If transport identity is used instead, bind the QUIC peer identity to the `sender` field and reject mismatches everywhere. Hub metering and rate limiting should key off the authenticated transport/session identity, not an unauthenticated payload field.
   (e) Which file and section it applies to: `PROTOCOL.md` Sections 0, 1.4, and 3.4-3.7 (lines 18-19, 95-109, 250-364); `mesh-dht/src/node.rs` (`handle_ping`, `handle_store`, `handle_find_node`, `handle_find_value`, and `update_routing_table`, lines 95-153 and 320-329); `docs/private/hub-payment-implementation.md` Section 3.1 (lines 141-186); `docs/PLAN-01-mesh-hub.md` Sections 5.2 and 6.2 (lines 453-459 and 555-558).

2. (a) Attack or vulnerability: QUIC peer authentication is effectively disabled in the reference transport. The client accepts any server certificate and any TLS 1.2/1.3 handshake signature, and the server is configured with `with_no_client_auth()`, so it does not authenticate clients at all. That allows active MITM/impersonation of peers and hubs, and it leaves the system unable to bind a transport connection to a mesh identity.
   (b) Severity: critical
   (c) Exploitability: trivial
   (d) Recommended mitigation: Replace the accept-all verifier with strict self-certifying verification. Require mutual authentication, extract the peer public key from the certificate using a real X.509 parser, verify the certificate and handshake signature, and bind that identity to the connection. Reject any request whose `sender` does not match the authenticated peer. Remove the "mesh.local" placeholder flow once verification is real.
   (e) Which file and section it applies to: `mesh-transport/src/tls.rs` (`MeshCertVerifier` and crypto config, lines 21-76 and 151-176); `mesh-transport/src/endpoint.rs` (`connect`, lines 82-89); `mesh-transport/src/connection.rs` (`peer_mesh_identity`, lines 49-60, currently unused by the request path); `PROTOCOL.md` Sections 3.1, 8.1, and 9.4 (lines 206-212, 695-705, 794-805).

## High

1. (a) Attack or vulnerability: Routing-table poisoning and eclipse are much easier than the spec claims. The implementation admits self-reported `sender_addr` data and arbitrary `NodeInfo` from responses without verifying reachability, then evicts the least-recently-seen entry immediately when a bucket is full. That defeats the spec's long-lived-node preference and gives an attacker a cheap way to crowd out honest peers.
   (b) Severity: high
   (c) Exploitability: trivial
   (d) Recommended mitigation: Only admit or refresh routing entries after a successful challenge/PING over the same authenticated connection. Implement the spec's "ping least-recently-seen before eviction" behavior, score peers by verified uptime, and add diversity constraints for hubs (for example by IP/subnet/ASN/operator). Treat unverified nodes as probationary until they pass reachability checks.
   (e) Which file and section it applies to: `PROTOCOL.md` Sections 4.3, 9.1, and 9.2 (lines 396-405 and 744-779); `mesh-dht/src/routing.rs` (`add_node`, lines 61-97); `mesh-dht/src/node.rs` (`handle_*`, `bootstrap`, and `lookup_value`, lines 95-177, 184-275, and 277-317).

2. (a) Attack or vulnerability: Revocation and key-rotation are specified as core compromise-response mechanisms, but the implementation does not enforce either one. A valid `core/revocation` descriptor is just stored as another opaque descriptor, and no code removes the revoked target or updates routing state for `core/key-rotation`. A compromised or withdrawn descriptor therefore remains discoverable until TTL expiry.
   (b) Severity: high
   (c) Exploitability: moderate
   (d) Recommended mitigation: Treat `core/revocation` and `core/key-rotation` as protocol-significant schemas, not ordinary opaque payloads. Parse and validate their payloads, maintain indexes by `target_id` and active identity, suppress revoked descriptors from reads, and persist revocation/rotation state across restarts.
   (e) Which file and section it applies to: `PROTOCOL.md` Sections 5.5, 7.1, and 7.2 (lines 585-605 and 648-682); `mesh-dht/src/storage.rs` (`store_descriptor` and `get_descriptors`, lines 105-221); repository-wide code search shows only schema constants for revocation/key rotation, with no runtime handling beyond `mesh-core/src/schema.rs`.

3. (a) Attack or vulnerability: The delegated `/api/v1/resolve/{descriptor_id}` design turns the hub into an SSRF pivot. A malicious descriptor can advertise a loopback, RFC1918, link-local, or metadata-service endpoint, and the hub will proxy the resolve on behalf of the caller. Because the admin API and portal are intended to live on the same machine, this can become a direct path into localhost-only services.
   (b) Severity: high
   (c) Exploitability: moderate
   (d) Recommended mitigation: Default-deny private, loopback, link-local, multicast, and metadata-service targets; require explicit operator allowlists for outbound resolve targets; isolate the resolve proxy in a restricted network namespace; and never allow the proxy to reach the admin API address range. If delegated resolve remains a paid feature, meter it after target validation, not before.
   (e) Which file and section it applies to: `PROTOCOL.md` Sections 5.2 and 5.4 (capability `endpoint.address` and direct resolve, lines 472-531 and 559-583); `docs/private/hub-payment-implementation.md` Section 7.1 (lines 382-399); `docs/PLAN-01-mesh-hub.md` Sections 6.1, 8.2, and 9.2 (admin API and localhost binding, lines 525-551, 631-699, and 735-748).

4. (a) Attack or vulnerability: Trial accounts can be recreated indefinitely with fresh DIDs, so the payment design currently hands out repeatable free usage to anyone willing to generate keys. The only proof required is DID-auth over a self-generated identity, and the protocol explicitly allows cheap ephemeral identities.
   (b) Severity: high
   (c) Exploitability: trivial
   (d) Recommended mitigation: Put a scarce anti-abuse signal in front of trial issuance: payment-method hold, invite, operator approval, email/phone verification, proof-of-work, proof-of-humanity, or risk-scored rate limits by IP/device/network. "One identity per account" is not meaningful when identities are free. Track prior trial grants across more than DID alone.
   (e) Which file and section it applies to: `docs/private/hub-payment-implementation.md` Section 2.2 and Section 3.3 (lines 63-96 and 227-238); `PROTOCOL.md` Section 7.3 and Section 9.1 (lines 684-689 and 744-766).

5. (a) Attack or vulnerability: Resource exhaustion remains cheap because publish limits are per publisher DID, while identities are cheap and hubs default to open/full-coverage storage. The current node implementation also stores every valid descriptor it receives without enforcing responsible-range checks or any capacity bound. An attacker can rotate identities and fill storage, memory, and indexing work with protocol-compliant descriptors.
   (b) Severity: high
   (c) Exploitability: trivial
   (d) Recommended mitigation: Enforce global and per-network admission controls in addition to per-identity limits: per-IP/per-subnet quotas, proof-of-work or stake for STORE, hard storage caps, responsible-range checks, backpressure when full, and operator policy defaults stricter than `open` + `full`. Run expiry eviction automatically, not as an optional helper.
   (e) Which file and section it applies to: `PROTOCOL.md` Sections 3.5, 4.1, and 9.3 (lines 290-297, 372-381, and 781-792); `docs/PLAN-01-mesh-hub.md` Sections 4.2, 5.3, 8.2, and 9.3 (lines 389-416, 463-475, 646-650 and 682-693, and 750-786); `mesh-dht/src/storage.rs` (per-publisher rate limiting and no capacity controls, lines 12-16, 36-58, and 105-166); `mesh-dht/src/node.rs` (`handle_store`, lines 111-125).

## Medium

1. (a) Attack or vulnerability: The responder honors attacker-supplied `max_results` directly, so `FIND_VALUE` can trigger oversized result construction and response amplification. The transport enforces a 1 MB limit when receiving frames, but there is no equivalent outbound cap before serializing and sending the response body.
   (b) Severity: medium
   (c) Exploitability: moderate
   (d) Recommended mitigation: Clamp incoming `max_results` to a server-side policy value, enforce an outbound serialized-body cap before sending, and add pagination/cursors for broad keys. Apply the same guardrails to the hub's planned HTTP discovery API.
   (e) Which file and section it applies to: `mesh-dht/src/node.rs` (`handle_find_value`, lines 151-177); `mesh-transport/src/connection.rs` (`MAX_FRAME_BODY` only enforced on receive, lines 11-12 and 85-129); `docs/private/hub-payment-implementation.md` Section 7.1 (`/api/v1/discover`, lines 382-399).

2. (a) Attack or vulnerability: DID canonicalization is inconsistent between the spec and the implementation. The spec says the method-specific ID is `multibase-base58btc(...)`, which implies a multibase prefix, but the Rust code emits plain base58 with no prefix. Once hubs start keying blocklists, tenant identities, and auth decisions on DID strings, multiple textual representations of the same key are likely.
   (b) Severity: medium
   (c) Exploitability: moderate
   (d) Recommended mitigation: Define one canonical textual DID form and implement a parser/normalizer now. Store and compare identity bytes as the security principal; use DID strings only as a display layer. Reject non-canonical textual forms at API boundaries.
   (e) Which file and section it applies to: `PROTOCOL.md` Section 1.3 (lines 84-93); `mesh-core/src/identity.rs` (`did`, lines 39-45); `docs/PLAN-01-mesh-hub.md` Sections 5.2, 6.1, and 9.3 (identity registration, DID-auth, and DID blocklists, lines 453-459, 543-558, and 755-776).

3. (a) Attack or vulnerability: Sequence/replay protection is not durable. The current store tracks highest-seen sequence numbers only in memory, so a restart forgets the replay floor. Older but still-valid descriptors can then be replayed back into storage until the newest publisher version is seen again.
   (b) Severity: medium
   (c) Exploitability: moderate
   (d) Recommended mitigation: Persist sequence watermarks by `(publisher, schema_hash, topic)` in durable storage and persist revocation state as well. On restart, reload those floors before admitting descriptors. For hubs, treat this as part of L2/L3 state, not ephemeral cache state.
   (e) Which file and section it applies to: `PROTOCOL.md` Section 2.2 step 8 and Section 2.3 (lines 181-197); `mesh-dht/src/storage.rs` (`sequences` only in memory, lines 61-70 and 136-149); `docs/PLAN-01-mesh-hub.md` Section 4.1 (storage layout does not include durable sequence floors, lines 372-387).

4. (a) Attack or vulnerability: DID-auth challenge-response is underspecified and not origin-bound. The docs say "random nonce" and "sign the challenge" but do not require an audience, hub identity, endpoint, action, expiry, or single-use tracking. That leaves room for replay, cross-endpoint confusion, and signing-oracle abuse once the admin/payment APIs are built.
   (b) Severity: medium
   (c) Exploitability: moderate
   (d) Recommended mitigation: Standardize the signed challenge format now: include hub DID/domain, endpoint/action, nonce, issued-at, expiry, and one-time token ID; store it server-side until consumed; and bind it to the session/connection that requested it. Reject reused or stale challenges.
   (e) Which file and section it applies to: `docs/PLAN-01-mesh-hub.md` Sections 5.2 and 6.2 (lines 453-459 and 553-558); `docs/private/hub-payment-implementation.md` Section 2.2 and Section 5.1 (lines 70-77 and 307-329); `PROTOCOL.md` Section 5.4 (`resolve-response.challenge`, lines 576-582).

5. (a) Attack or vulnerability: Key material handling is not hardened. The docs specify filesystem keypair paths and backups but do not require restrictive permissions, and the CLI writes raw secret keys with ambient filesystem defaults. The CLI also prints the secret key in hex when no path is provided.
   (b) Severity: medium
   (c) Exploitability: moderate
   (d) Recommended mitigation: Create key files with `0600` permissions, refuse to use overly permissive key files, prefer OS key stores or HSM-backed signing for hubs, and remove secret-key printing from the default UX. Keep secret material out of logs and minimize raw-byte exposure APIs.
   (e) Which file and section it applies to: `docs/PLAN-01-mesh-hub.md` Sections 8.2 and 10.2 (lines 633-639 and 816-836); `mesh-node/src/main.rs` (`load_or_generate_keypair` and `cmd_identity`, lines 113-128 and 442-455).

6. (a) Attack or vulnerability: The observability design leaks tenant and publisher metadata in ways that make hub-operator profiling easy and accidental exposure costly. Per-tenant labeled metrics, structured logs containing `tenant_id` and `publisher`, and the payment dashboard's per-identity usage model create a detailed behavioral record with no minimization or retention guidance.
   (b) Severity: medium
   (c) Exploitability: moderate
   (d) Recommended mitigation: Default to aggregate metrics, avoid high-cardinality tenant/publisher labels, redact or hash identifiers where possible, protect metrics endpoints as sensitive admin surfaces, and define retention/minimization rules before implementing billing/observability.
   (e) Which file and section it applies to: `PROTOCOL.md` Section 9.4 (lines 794-805); `docs/PLAN-01-mesh-hub.md` Sections 7.1 and 7.2 (lines 564-606); `docs/private/hub-payment-implementation.md` Sections 3.2, 5.2, and 9 (lines 199-208, 331-339, and 483-488).

## Low

1. (a) Attack or vulnerability: Algorithm-ID allocation by "publish a descriptor and let adoption win" creates an avoidable downgrade/confusion surface once multiple signature/hash algorithms are actually live. Different implementations could assign different meanings to the same algorithm byte or choose different minimum-security policies.
   (b) Severity: low
   (c) Exploitability: theoretical
   (d) Recommended mitigation: Move algorithm-ID governance out of social convergence and into a fixed registry, signed registry document, or explicit compatibility policy with minimum-security floors. At minimum, define how conflicting registrations are rejected rather than socially "winning."
   (e) Which file and section it applies to: `PROTOCOL.md` Section 1.1 (lines 37-57).

2. (a) Attack or vulnerability: The deferred voucher design is a bearer instrument by default. It is explicitly transferable and not bound to a holder identity, so theft or interception would equal spend if this design is implemented as written.
   (b) Severity: low
   (c) Exploitability: theoretical
   (d) Recommended mitigation: If vouchers are ever built, bind them to a tenant or holder DID, include audience restrictions, require authenticated redemption, and treat transferability as an opt-in feature rather than the default.
   (e) Which file and section it applies to: `docs/private/hub-payment-implementation.md` Section 2.3 (lines 98-128).
