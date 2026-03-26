# Make Money With Your Idle GPU

Turn your gaming PC into a passive income machine. Share your GPU on the MARS mesh network and get paid per inference request.

## How Much Can You Earn?

| GPU | Models | Est. Earnings (24/7) |
|-----|--------|---------------------|
| RTX 4090 | Llama 3.3 70B (Q4) | $5-15/day |
| RTX 3090 | Mistral 7B, Llama 8B | $2-8/day |
| RTX 4070 | Mistral 7B, Phi-3 | $1-5/day |
| RTX 3060 12GB | Llama 8B (Q4) | $0.50-3/day |

*Earnings depend on demand and your pricing. Early providers get the most traffic as the network grows.*

## Quick Start (5 minutes)

### 1. Install Ollama + a model

```bash
curl -fsSL https://ollama.com/install.sh | sh
ollama pull llama3.3    # or any model that fits your VRAM
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

- **$0.10-0.20/1K tokens** — competitive with cloud providers
- **$0.05/1K tokens** — undercutting the market, high volume
- **$0.00** — free tier to build reputation (recommended for first week)

Start free to build reputation and get reviews, then set your price.

## Requirements

- NVIDIA GPU with 8GB+ VRAM (or AMD with ROCm)
- Ollama installed
- Stable internet connection
- Port forwarding or ngrok (auto-configured by provider agent)
