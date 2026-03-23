# Hub Payment & Entitlement — Private Implementation Design

**Status:** Design Draft
**Date:** 2026-03-22
**Scope:** PurposeBot hub implementation only. NOT part of the open source spec.
**Depends on:** PLAN-01-mesh-hub.md (Sections 5, 6)

---

## 1. Overview

This document describes how the PurposeBot hub collects payment, tracks
entitlement, and enforces access for paying vs. free vs. unregistered
clients. The open source hub spec (PLAN-01) defines the metering
infrastructure (Mesh Units, tenant model, quotas) but is deliberately
payment-agnostic. This document fills in the payment side.

### 1.1 Design Constraints

1. Must work for human operators signing up via web portal AND autonomous
   agents signing up programmatically.
2. No blockchain dependency. Fiat-first, crypto-optional later.
3. Wire-speed enforcement — no per-request I/O for auth or billing.
4. Graceful degradation — if the billing system is down, the hub continues
   serving (metering locally, reconciling later).

---

## 2. Payment Channels

### 2.1 Human-Initiated (Web Portal)

Standard SaaS signup flow:

```
1. Operator visits hub.purposebot.ai
2. Creates account (email + password, or OAuth)
3. Selects tier (Free / Pro / Enterprise)
4. Pro/Enterprise: enters payment via Stripe Checkout
5. Account created → redirected to dashboard
6. Registers agent DIDs via dashboard or admin API
```

**Stripe integration:**
- Stripe Checkout for initial signup
- Stripe Billing for recurring subscriptions (monthly)
- Stripe Usage Records for overage billing (MU usage beyond plan)
- Stripe Webhooks for payment lifecycle events (payment succeeded,
  payment failed, subscription canceled)

**Tier mapping:**

| Tier | Monthly Price | Included MUs | Overage Rate |
|------|-------------|-------------|-------------|
| Free | $0 | 100,000 MU | N/A (hard cap) |
| Pro | $49 | 5,000,000 MU | $0.01 / 1,000 MU |
| Enterprise | $499 | 100,000,000 MU | $0.005 / 1,000 MU |

MU budgets reset on billing cycle. Unused MUs do not roll over.

### 2.2 Agent-Initiated (Programmatic)

Autonomous agents need to sign up without a human in the loop. Flow:

```
1. Agent discovers hub via mesh (FIND_VALUE at infrastructure/hub)
2. Agent reads hub's capability descriptor:
   - Sees pricing in constraints.pricing
   - Sees admin API address in endpoint
3. Agent calls POST /api/v1/account/register:
   {
     "identity": <agent's Identity>,
     "tier": "pro"
   }
4. Hub returns DID-auth challenge (nonce)
5. Agent signs challenge with private key
6. Hub verifies signature → account created (trial status)
7. Hub returns:
   {
     "account_id": "...",
     "status": "trial",
     "trial_mu": 10000,
     "payment_url": "https://hub.purposebot.ai/pay/{account_id}",
     "payment_methods": ["stripe", "mesh-voucher"]
   }
8. Agent (or its operator) completes payment:
   Option A: Visit payment_url (human-assisted)
   Option B: POST /api/v1/account/deposit with payment token
```

**Trial accounts:**
- New agent-initiated accounts get a 10,000 MU trial budget
- Trial expires after 7 days or MU exhaustion, whichever comes first
- No payment method required for trial
- Trial limits: 1 identity per account, free-tier rate limits
- Converts to paid on first successful payment

### 2.3 Future: Mesh Vouchers (Agent-Native Payment)

When machine-to-machine payments become viable, vouchers provide a
protocol-aligned payment method:

```
Voucher {
  hub_identity:   Identity      // which hub issued this
  tenant_id:      uuid          // which account it's for
  mu_amount:      u64           // Mesh Units granted
  issued_at:      u64           // timestamp
  expires_at:     u64           // expiry timestamp
  nonce:          [u8; 16]      // replay protection
  signature:      bytes         // hub signs all above fields
}
```

Properties:
- Issued by the hub in exchange for payment (any method)
- Presented by the agent with protocol requests (in a CBOR extension
  field, or via admin API pre-registration)
- Verifiable offline — hub checks its own signature, no database lookup
- Single-use or balance-based (hub tracks nonce to prevent replay)
- Transferable — an agent can give its voucher to another agent

Vouchers are NOT part of the mesh protocol. They're an HTTP-level
construct between the agent and the hub's admin API. The mesh protocol
messages themselves don't change — the hub identifies the agent by its
sender Identity and maps it to the voucher's tenant account.

**Deferred.** Implement when there's demand for fully autonomous agent
onboarding without any human payment step.

---

## 3. Entitlement Enforcement

### 3.1 The Hot Path

Every protocol message hits this path. It must be microsecond-fast.

```
                    ┌──────────────────────┐
  QUIC stream ──────│ Extract sender       │
                    │ Identity from frame  │
                    └─────────┬────────────┘
                              │
                    ┌─────────▼────────────┐
                    │ L0: Connection cache  │ ← per-connection, set on first message
                    │ (Identity → Tenant)   │   avoids even the hashmap lookup on
                    └─────────┬────────────┘   subsequent messages on same connection
                              │ miss
                    ┌─────────▼────────────┐
                    │ L1: Tenant cache      │ ← in-memory HashMap<Identity, TenantCtx>
                    │ (all registered DIDs) │   populated from SQLite on startup
                    └─────────┬────────────┘   updated on tenant changes via channel
                              │ miss
                    ┌─────────▼────────────┐
                    │ Unregistered client   │ ← global rate limit, lowest priority
                    └─────────┬────────────┘
                              │
                    ┌─────────▼────────────┐
                    │ TenantCtx:           │
                    │  - tier              │
                    │  - rate_limiter      │ ← token bucket (in-memory, per-tenant)
                    │  - mu_balance        │ ← AtomicI64 (in-memory, flushed async)
                    │  - billing_status    │
                    └─────────┬────────────┘
                              │
                    ┌─────────▼────────────┐
                    │ Check billing_status  │
                    │ suspended? → reject   │
                    └─────────┬────────────┘
                              │
                    ┌─────────▼────────────┐
                    │ Check rate limiter    │
                    │ exceeded? → reject    │
                    └─────────┬────────────┘
                              │
                    ┌─────────▼────────────┐
                    │ Deduct MU (atomic)    │
                    │ balance ≤ 0?          │
                    │  soft: reject STORE   │
                    │  hard: reject all     │
                    └─────────┬────────────┘
                              │
                    ┌─────────▼────────────┐
                    │ Process request       │
                    └──────────────────────┘
```

**Zero I/O on the hot path.** Everything is in-memory:
- Connection cache: `HashMap` on the QUIC connection state
- Tenant cache: `DashMap<Identity, Arc<TenantCtx>>` (lock-free concurrent map)
- Rate limiter: `governor` crate (in-memory token bucket per tenant)
- MU balance: `AtomicI64` (lock-free atomic)

### 3.2 Cold Path (Async Reconciliation)

A background task handles persistence and billing:

```
Every 5 seconds:
  - Flush MU balance decrements to SQLite
  - Check for tenants approaching quota (>80%)
    → emit webhook notification

Every 60 seconds:
  - Sync tenant cache from SQLite (picks up new registrations,
    tier changes, billing status updates from web portal)
  - Report per-tenant MU usage to Stripe (Usage Records)

Every billing cycle (monthly):
  - Reset free-tier MU budgets
  - Generate usage invoices for paid tiers (Stripe handles this)
  - Suspend accounts with failed payments (after grace period)
```

**Crash recovery:**
If the hub crashes between flushes, at most 5 seconds of MU decrements
are lost. This means tenants get a small amount of free usage after a
crash. Acceptable — the alternative (write-through on every request)
kills throughput.

On restart:
1. Load tenant cache from SQLite
2. All MU balances are at their last-flushed values
3. Resume normal operation

### 3.3 Client Classification

| Category | Identification | Rate Limit | MU Budget | Eviction | STORE | FIND_VALUE |
|----------|---------------|------------|-----------|----------|-------|------------|
| Enterprise | Registered DID, enterprise tier | 1000/s | Per plan | Last | Yes | Yes |
| Pro | Registered DID, pro tier | 100/s | Per plan | After enterprise | Yes | Yes |
| Free | Registered DID, free tier | 10/s | 100K/month | After pro | Yes | Yes |
| Trial | Registered DID, trial status | 10/s | 10K one-time | After free | Yes | Yes |
| Unregistered | Unknown DID | 2/s | N/A (no budget) | First | Yes (lowest priority) | Yes |

All categories can query. All categories can store. The difference is
priority, rate, and reliability of service.

---

## 4. Stripe Integration Detail

### 4.1 Webhook Events

| Stripe Event | Hub Action |
|-------------|------------|
| `checkout.session.completed` | Activate account, set tier, load MU budget |
| `invoice.paid` | Reset MU budget for new billing cycle |
| `invoice.payment_failed` | Mark account as payment_failed, send warning |
| `customer.subscription.deleted` | Downgrade to free tier after grace period |
| `customer.subscription.updated` | Update tier and quotas in tenant cache |

### 4.2 Usage-Based Billing

For Pro and Enterprise tiers, overage beyond included MUs is billed
via Stripe Usage Records:

```
Every 60 seconds:
  for each paid tenant with usage > 0 since last report:
    stripe.subscription_items.create_usage_record(
      subscription_item_id,
      quantity: mu_consumed_since_last_report,
      timestamp: now,
      action: "increment"
    )
```

Stripe aggregates usage records and adds overage charges to the next
invoice automatically. The hub doesn't calculate overage pricing — Stripe
handles it based on the plan's metered pricing configuration.

### 4.3 Pricing Configuration

Stripe Products:

```
Product: "Mesh Hub - Pro"
  Price: $49/month (recurring, fixed)
  Metered Price: $0.01 per 1,000 MU (usage-based, overage)

Product: "Mesh Hub - Enterprise"
  Price: $499/month (recurring, fixed)
  Metered Price: $0.005 per 1,000 MU (usage-based, overage)
```

### 4.4 Grace Periods

| Event | Grace Period | Action After Grace |
|-------|-------------|-------------------|
| Payment failed | 7 days | Downgrade to free tier quotas (account preserved) |
| Subscription canceled | 30 days | Downgrade to free tier (descriptors preserved) |
| Account delinquent (2+ failed) | 14 days | Suspend (STORE rejected, FIND_VALUE at 2/s) |

Suspended accounts are never deleted. Descriptors remain stored (at
lowest eviction priority) for 90 days. Reactivation restores full
service immediately upon successful payment.

---

## 5. Admin API Extensions (Private)

These endpoints extend the open source admin API (PLAN-01 Section 6)
with payment-specific functionality.

### 5.1 Registration & Payment

```
POST   /api/v1/account/register      — create account (agent-initiated)
  Request:  { identity: Identity }
  Response: { account_id, status: "trial", trial_mu, payment_url, payment_methods }
  Auth:     DID-auth challenge-response

POST   /api/v1/account/deposit        — add MU balance via payment
  Request:  { method: "stripe", token: "..." }
  Response: { mu_added, new_balance }
  Auth:     DID-auth

GET    /api/v1/account/billing         — billing status and history
  Response: { tier, mu_balance, mu_used_this_cycle, next_reset, payment_status,
              invoices: [...] }
  Auth:     DID-auth

POST   /api/v1/account/upgrade         — change tier
  Request:  { tier: "pro" | "enterprise" }
  Response: { redirect_url } (Stripe Checkout for payment method)
  Auth:     DID-auth
```

### 5.2 Operator Billing Dashboard

```
GET    /api/v1/billing/summary         — total revenue, MU consumption, tenant breakdown
GET    /api/v1/billing/tenants         — per-tenant billing status and usage
GET    /api/v1/billing/revenue         — revenue over time (daily/weekly/monthly)
POST   /api/v1/billing/adjust          — manual MU credit/debit for a tenant
  Request:  { tenant_id, mu_delta, reason }
```

---

## 6. Web Portal

The hub's web frontend for human operators. Separate deployment from the
hub binary — communicates with the hub via admin API.

### 6.1 Pages

```
/                           — landing page, pricing, signup CTA
/signup                     — Stripe Checkout → account creation
/login                      — email/password or OAuth
/dashboard                  — account overview, MU usage gauge, quick actions
/dashboard/identities       — manage registered DIDs
/dashboard/descriptors      — view stored descriptors, status
/dashboard/usage            — MU consumption over time, per-identity breakdown
/dashboard/billing          — current plan, invoices, payment method, upgrade
/dashboard/api-keys         — (future) manage API keys for delegated access
```

### 6.2 Tech Stack

- **Frontend:** lightweight — Astro or plain HTML + htmx. No SPA framework
  needed for a dashboard this simple.
- **Backend:** the hub's admin API. The portal is a thin UI layer over
  existing endpoints.
- **Auth:** session-based (cookie) for the portal, translating to DID-auth
  for admin API calls. The portal holds a signing key that acts on behalf
  of the operator.
- **Hosting:** same server as the hub, different port. Or separate deploy
  behind the same domain with path-based routing.

---

## 7. Delegated Routing (Premium Service)

Lightweight clients (`mesh-client`) that can't do DHT lookups can query
the hub's HTTP API instead. This is a key paid feature — it turns the
hub into a capability search engine.

### 7.1 HTTP Discovery API

```
GET /api/v1/discover?type={capability_type}&limit={n}
  Returns: matching capability descriptors (JSON)
  Auth: DID-auth or API key
  Cost: 1 MU per query

GET /api/v1/discover?type={capability_type}&available_now=true&max_price=0.001
  Returns: filtered results
  Auth: DID-auth or API key
  Cost: 1 MU per query

GET /api/v1/resolve/{descriptor_id}
  Returns: live status from provider (hub proxies the RESOLVE)
  Auth: DID-auth or API key
  Cost: 5 MU (hub makes outbound QUIC connection on your behalf)
```

### 7.2 Value Proposition

For agents that don't want to (or can't) run a mesh node:
- No QUIC implementation needed — plain HTTPS
- No DHT participation — no routing table, no storage overhead
- No bootstrapping — one HTTP endpoint gets you the whole mesh
- Faster than DIY lookups — hub has full DHT coverage, answers from cache

This is the Infura/Alchemy model applied to capability discovery. It's
why someone pays for a hub instead of running their own node.

---

## 8. Descriptor Replication (Premium Service)

Tenants on Pro and Enterprise tiers get descriptor replication to peer hubs
for high availability.

### 8.1 Replication Model

```
Tenant registers descriptors on Hub A (home hub)
Hub A replicates to Hub B and Hub C (replication peers)
  → Hub B and Hub C store descriptors with a "replica" tag
  → Replicas are served on FIND_VALUE like any other descriptor
  → If Hub A goes down, Hub B and Hub C still serve the descriptors
```

### 8.2 Replication Configuration

Per-tenant, configurable:

| Tier | Replication Factor | Replication Targets |
|------|-------------------|-------------------|
| Free | 0 (DHT only) | N/A |
| Pro | 2 (home + 1 peer) | Auto-selected by geographic diversity |
| Enterprise | 3 (home + 2 peers) | Operator-chosen or auto-selected |

### 8.3 Replication Protocol

Hub-to-hub replication uses the existing STORE mechanism:
1. Home hub STOREs the descriptor to replication peer hubs.
2. Replication peers store it with a `replica_of: tenant_id` tag.
3. On republish (TTL/2), home hub re-STOREs to replication peers.
4. If home hub misses a republish cycle, replication peers retain the
   descriptor until its TTL expires (grace period).
5. Replication peers do NOT count replica storage against their own
   tenant quotas — it's a service agreement between hubs.

### 8.4 Business Model

Replication is a hub-to-hub service. Hub A pays Hub B for replica storage.
Settlement is between hub operators — could be reciprocal (I store yours,
you store mine), cash, or any other arrangement. The protocol doesn't
care.

---

## 9. Build Phases (Private Implementation)

### Phase A: MVP (status: planned)
- [ ] Web portal — landing page, Stripe Checkout signup, basic dashboard
- [ ] Stripe integration — subscription creation, webhook handling
- [ ] MU budget loading on payment success
- [ ] MU balance flush to SQLite (5s interval)
- [ ] Stripe Usage Records reporting (60s interval)
- [ ] Trial account flow (agent-initiated registration)
- [ ] Grace period handling for failed payments

### Phase B: Delegated Routing (status: planned)
- [ ] HTTP discovery API (`/api/v1/discover`)
- [ ] HTTP resolve proxy (`/api/v1/resolve`)
- [ ] MU metering for HTTP API calls
- [ ] Rate limiting per API key / DID

### Phase C: Replication (status: planned)
- [ ] Replication peer selection (geographic diversity)
- [ ] STORE-based replication to peer hubs
- [ ] Replica retention and TTL tracking
- [ ] Replication health monitoring
- [ ] Per-tenant replication configuration in dashboard

### Phase D: Polish (status: planned)
- [ ] Usage analytics dashboard (per-identity MU breakdown)
- [ ] Billing history and invoice download
- [ ] API key management (alternative to DID-auth for HTTP API)
- [ ] Voucher system for agent-native payment
- [ ] Multi-region hub deployment guide
