# Make Money With Your Idle GPU

Turn your gaming PC into a passive income machine. Share your GPU on the MARS mesh network and get paid per inference request.

## How Much Can You Earn?

| GPU | ~Throughput | Break-even | Suggested Price | Est. Earnings (24/7, 50% util) |
|-----|-----------|-----------|----------------|-------------------------------|
| RTX 4090 | 25 tok/s | $0.0009/1K | $0.0027/1K | $3-5/day |
| RTX 3090 | 15 tok/s | $0.0011/1K | $0.0033/1K | $1.50-3/day |
| RTX 4070 | 12 tok/s | $0.0007/1K | $0.0021/1K | $0.80-2/day |
| A100 | 40 tok/s | $0.0056/1K | $0.0167/1K | $8-15/day |
| H100 | 80 tok/s | $0.0069/1K | $0.0208/1K | $15-30/day |

*Earnings depend on demand, utilization, and model. Prices calculated from throughput benchmarks and electricity/rental costs. Early providers get the most traffic.*

## Quick Start (5 minutes)

### 1. Install Ollama + a model

```bash
curl -fsSL https://ollama.com/install.sh | sh
ollama pull llama4    # or any model that fits your VRAM
```

### 2. Create a Stripe account

Go to https://connect.stripe.com/express_dashboard_link (or we'll generate one for you).

### 3. Start providing

```bash
pip install httpx

# Start the mesh gateway (connects to the live network)
./mesh-gateway --seed 5.161.53.251:4433 --listen 127.0.0.1:3000 &

# Share your GPU and set your price
python provider.py \
    --gateway http://localhost:3000 \
    --price 0.15 \
    --stripe-account acct_YOUR_STRIPE_ID
```

That's it. Your GPU is now discoverable worldwide. When agents send inference requests, you get paid directly to your Stripe account.

### 4. Monitor earnings

```bash
# Check your provider status
curl http://localhost:3000/v1/provider/status

# View Stripe dashboard
# https://dashboard.stripe.com
```

## How It Works

1. You run the provider agent — it auto-detects your GPU and models
2. It publishes your capabilities to the MARS mesh with your price
3. AI agents discover you when they need inference
4. The mesh gateway handles payment (Stripe) and proxies the request
5. Your Ollama instance runs the inference
6. You get paid. The platform takes 10%.

## Pricing Guide

The provider agent calculates pricing based on your GPU's actual throughput and electricity costs:

- **Break-even** — your real cost per M tokens (electricity/rental only)
- **2-3x break-even** — competitive, good volume
- **5x break-even** — premium, lower volume but higher margin
- **$0.00** — free tier to build reputation (recommended for first week)

For reference, cloud API pricing:
- OpenAI GPT-4o: $0.005/M tokens
- Together AI Llama 70B: $0.0009/M tokens
- Groq Llama 70B: $0.0006/M tokens

Your break-even on a consumer GPU is typically $0.0007-0.0015/M tokens — well below cloud APIs. Even at 3x margin you're undercutting everyone.

## Requirements

- NVIDIA GPU with 8GB+ VRAM (or AMD with ROCm)
- Ollama installed
- Stable internet connection
- Port forwarding or ngrok (auto-configured by provider agent)
