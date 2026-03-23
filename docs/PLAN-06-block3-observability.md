# PLAN-06: Block 3 — Observability + Security #5, #8

**Status:** in-progress
**Depends on:** Block 2 (multi-tenant) ✅
**Parallel streams:** 2

---

## Stream A: Observability (Phase 3) — status: planned

Constrained by Security #8: no tenant labels in default Prometheus metrics,
protect /metrics as admin-only, hash/redact identifiers in logs.

### A1: Prometheus metrics — mesh-hub
- Add `metrics` + `metrics-exporter-prometheus` crates (or `prometheus-client`)
- Define counters/gauges/histograms:
  - DHT: queries_total, stores_total, query_latency_seconds
  - Storage: descriptors_total, storage_bytes, evictions_total
  - Peering: peers_connected, gossip_rounds_total
  - Rate limiting: rate_limited_total (by operation)
  - Transport: connections_active
- Emit metrics in hooks.rs (post_store, post_query) and background tasks
- Security #8: aggregate only — no tenant_id or publisher labels by default

### A2: Metrics endpoint — mesh-hub/src/admin.rs
- GET /metrics → Prometheus text format
- Protected by operator_token auth (same as other admin routes)
- Security #8: behind auth, not public

### A3: Structured logging — mesh-hub
- Ensure all tracing calls include structured fields
- Add request_id, latency_ms, operation fields to protocol handler
- No raw DIDs in info-level logs (hash to 8-char prefix for privacy)

### A4: Config — mesh-hub/src/config.rs
- ObservabilityConfig: metrics_enabled, log_format (json|text), log_level

---

## Stream B: SSRF Prevention (Security #5) — status: planned

### B1: Address validation — mesh-hub
- Create address validation utility: reject private/loopback/RFC1918
- Validate before any outbound connection (peering, resolve)
- Allowlist in config for operator-approved internal addresses

---

## Phase 4 (Hardening) — deferred to next session

Phase 4 is testing/documentation, not implementation:
- Load testing harness (100K descriptors, 1K tenants)
- Security audit against PLAN-01 checklist
- Backup/recovery testing
- Operator/onboarding documentation
- Security #9 (voucher binding) — deferred low priority
