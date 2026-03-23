# PLAN-05: Block 2 — Multi-Tenant + Rate Limiting

**Status:** in-progress
**Depends on:** Block 1 (hub peering + DHT hardening) ✅
**Parallel streams:** 2

---

## Stream A: Tenant System + DID-Auth + Quotas — status: planned

### A1: DID-Auth Challenge (Security #7) — mesh-hub/src/auth.rs (new)
- DIDAuthChallenge struct: id, nonce, hub_did, endpoint, action, issued_at, expiry
- Challenge storage: SQLite table `did_auth_challenges`
- create_challenge() → stores and returns challenge
- verify_challenge() → validates signature, checks expiry, consumes (single-use)

### A2: Identity Registration with Verification — mesh-hub/src/admin.rs
- POST /api/v1/tenants/:id/identities/challenge → returns challenge
- POST /api/v1/tenants/:id/identities/verify → validates signed challenge, registers identity
- Replace current direct registration with challenge-response flow

### A3: MU Metering — mesh-hub/src/tenant.rs
- Add mu_balance, mu_limit to Tenant struct (SQLite schema migration)
- MU cost table: STORE=10, UPDATE=5, FIND_VALUE=1, FIND_NODE=1, retention=1/day
- deduct_mu(tenant_id, cost) → returns Ok/Err(InsufficientBalance)
- increment_usage(tenant_id, descriptors, bytes) → update counters

### A4: Quota Enforcement — mesh-hub/src/policy.rs
- check_quotas(tenant, descriptor) → enforce max_descriptors, max_storage_bytes
- check_mu_budget(tenant, cost) → enforce MU limits
- Integrate with existing check_store flow

### A5: Admin API Auth — mesh-hub/src/admin.rs
- Bearer token auth middleware (compare to config.operator_token)
- GET /api/v1/tenants/:id/usage → MU consumption, quota status
- PATCH /api/v1/tenants/:id/quota → operator adjusts limits

### A6: Config Updates — mesh-hub/src/config.rs
- operator_token: Option<String>
- MU cost table: MuCosts struct with per-operation costs
- Default tier configurations with MU budgets

---

## Stream B: Rate Limiting — status: planned

### B1: Rate Limiter Module — mesh-hub/src/rate_limit.rs (new)
- HubRateLimiter struct with sliding window per IP and per Identity
- check_ip(ip, operation) → Ok/Err
- check_identity(identity, operation) → Ok/Err
- record(ip, identity, operation) → track usage
- cleanup() → evict stale entries (background task)

### B2: Rate Limit Config — mesh-hub/src/config.rs
- RateLimitConfig: per-IP and per-identity thresholds per operation type
- Defaults: connections=60/min/IP, stores=10/min/IP, queries=100/min/IP

### B3: Hook Integration — mesh-hub/src/hooks.rs
- Add rate_limiter field to HubProtocolHook
- pre_store: check IP + identity rates before policy
- pre_query: check IP + identity rates

### B4: Eviction Under Pressure — mesh-hub/src/storage
- When hub storage is full, evict by: expired first, then LRU by last-queried
- Tenant priority: free tier evicted before paid tier

---

## Agent Assignment

| Agent | Files Created | Files Modified |
|-------|-------------|---------------|
| **Tenant+Auth** | auth.rs | tenant.rs, admin.rs, policy.rs, config.rs |
| **Rate Limiting** | rate_limit.rs | hooks.rs, config.rs |

Potential overlap in config.rs — agents add different sections, merge manually.
