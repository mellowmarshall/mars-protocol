# MARS Payment Architecture — Purposebot Integration

## Overview

MARS handles discovery. Purposebot handles commerce. They compose cleanly:

```
┌─────────────┐     ┌──────────────┐     ┌─────────────────┐     ┌──────────────┐
│  Consumer    │     │  MARS Mesh   │     │   Purposebot    │     │ GPU Provider │
│  Agent       │────▶│  (discover)  │────▶│  (pay + escrow) │────▶│ (inference)  │
└─────────────┘     └──────────────┘     └─────────────────┘     └──────────────┘
                    Kademlia DHT          Stripe/x402/escrow      Ollama + ngrok
```

## Flow

### Provider Onboarding (one-time)
1. Provider creates purposebot agent identity (JWT keypair)
2. Provider registers as producer via `POST /v1/commerce/producers/register`
3. Provider completes Stripe Connect onboarding
4. Provider runs `provider.py --purposebot-agent-id agent_xxx --price 0.15`
5. Descriptor published to MARS mesh with `purposebot_agent_id` field

### Consumer Inference (per-request)
1. Agent discovers GPU provider via MARS: `client.discover("compute/inference")`
2. Agent reads descriptor: `price=0.15/1K tokens, purposebot_agent_id=agent_xxx`
3. Agent calls purposebot payment API:
   - `POST /v1/commerce/orders` (quote)
   - `POST /v1/payments/{id}/authorize` (pay)
4. Purposebot returns fulfillment token (signed JWT, 60-300s expiry)
5. Agent sends inference request to provider endpoint with fulfillment token
6. Provider verifies token, runs inference, returns result
7. Provider submits fulfillment proof to purposebot
8. Purposebot releases escrow to provider's Stripe account

### Payment Methods
- **Stripe** (credit card) — via Stripe Connect destination charges
- **x402 USDC** (crypto) — via purposebot's x402 facilitator on Base
- **MU Credits** (platform credits) — via MARS hub MU metering

## Purposebot Components Used

| Component | Purpose |
|-----------|---------|
| AgentIdentity + JWT | Provider and consumer authentication |
| Producer Registration | Provider onboarding with Stripe Connect |
| Listings | GPU service definitions (pricing, models, specs) |
| Payment State Machine | quote → authorize → execute → settle |
| Escrow | Hold funds until fulfillment verified |
| Fulfillment Proof | Provider submits inference result hash |
| Payment Ledger | Tamper-evident audit trail |
| Spend Capabilities | Consumer budget limits and constraints |
| Replay Guards | Prevent double-spend on fulfillment tokens |

## Descriptor Schema (Extended for Payments)

```json
{
  "type": "compute/inference/text-generation",
  "endpoint": "https://abc123.ngrok.io",
  "params": {
    "name": "Llama 4 Scout (RTX 4090)",
    "model": "llama4:latest",
    "gpu": "NVIDIA GeForce RTX 4090",
    "vram_mb": 24576,
    "price_per_mtok": 0.15,
    "currency": "USD",
    "accepts_payment": true,
    "purposebot_agent_id": "agent_xxx",
    "payment_methods": ["stripe", "x402"],
    "region": "us-east",
    "ollama_api": "https://abc123.ngrok.io/api/generate",
    "openai_compat": "https://abc123.ngrok.io/v1/chat/completions"
  }
}
```

## What MARS Builds (Thin Adapter)

1. **Gateway payment proxy** — `/v1/inference` endpoint that:
   - Discovers provider on mesh
   - Creates order on purposebot
   - Proxies inference request with fulfillment token
   - Returns result to consumer

2. **Provider agent** — `provider.py --purposebot-agent-id` flag that:
   - Includes purposebot agent ID in mesh descriptors
   - Verifies fulfillment tokens on incoming requests
   - Submits fulfillment proofs after inference

3. **Consumer SDK** — `MeshClient.infer(type, prompt, payment_method)` that:
   - Discovers cheapest/closest provider
   - Handles payment via purposebot
   - Returns inference result

## What Purposebot Provides (Already Built)

- Stripe Connect Express onboarding
- Payment state machine (idempotent)
- Escrow with configurable auto-release
- Agent identity + JWT proof verification
- Anti-fraud (replay guards, hash chain, spend seal)
- x402 USDC alternative payment method
- Agent certificates with trust scoring
- Producer registration + payout management
- Order lifecycle with fulfillment tracking
- Compliance audit trail
