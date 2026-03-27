# MARS GPU Provider Agent

Turn any machine with a GPU into a mesh inference provider. One command to set up, one command to make it permanent.

## Quick Start

```bash
# Install Ollama (if you haven't already)
curl -fsSL https://ollama.com/install.sh | sh
ollama pull llama4

# Start a local gateway connected to the MARS network
./mesh-gateway --seed 5.161.53.251:4433 --listen 127.0.0.1:3000 &

# Interactive setup (first time only)
pip install httpx
python provider.py

# Install as a permanent service — survives reboot, auto-restarts on crash
python provider.py --install
```

That's it. Your GPU is now on the MARS mesh permanently.

## Running as a Service

Descriptors on the mesh have a TTL (time-to-live). If you just run `provider.py` in a terminal and close it, your GPU will disappear from the mesh after the TTL expires.

**`--install` solves this.** It creates a system service that:
- Starts automatically on boot
- Restarts automatically if it crashes
- Uses 24-hour TTL with 6-hour refresh (survives 18 hours of downtime)
- Runs without a terminal window
- No sudo required

```bash
python provider.py --install    # Install and start the service
python provider.py --status     # Check if it's running
python provider.py --uninstall  # Stop and remove the service
```

**Linux:** Creates a systemd user service (`systemctl --user`)
**macOS:** Creates a launchd agent (`~/Library/LaunchAgents/`)
**Windows:** Prints instructions for Task Scheduler / WSL

## What Gets Published

For each model on your GPU, a descriptor goes on the mesh:

```
type:     compute/inference/text-generation
endpoint: https://abc123.ngrok.io
params:
  name:              "llama4:latest (NVIDIA GeForce RTX 3090)"
  model:             "llama4:latest"
  gpu:               "NVIDIA GeForce RTX 3090"
  vram_mb:           24576
  price_per_mtok:    0.00
  region:            "us-east"
  ollama_api:        "https://abc123.ngrok.io/api/generate"
  openai_compat:     "https://abc123.ngrok.io/v1/chat/completions"
```

Other agents discover it with:
```python
providers = client.discover("compute/inference/text-generation")
```

## Prerequisites

- [Ollama](https://ollama.com) installed and running
- Python 3.9+ with `httpx` (`pip install httpx`)
- A mesh gateway running (or connect to the public network)
- ngrok (optional, auto-detected — makes your GPU reachable from anywhere)

## Setup Flow

On first run, `provider.py` walks you through:

1. **GPU detection** — auto-detects NVIDIA/AMD hardware and VRAM
2. **Model selection** — lists installed Ollama models, lets you pick which to share
3. **Pricing** — suggests rates based on your GPU's throughput and electricity costs
4. **Stripe setup** — opens browser for payment onboarding (optional, skip for free tier)
5. **Region detection** — auto-detects your location via IP geolocation
6. **Config saved** — writes `mars-provider.json`, never asks again

## Networking

By default, Ollama only listens on localhost. The provider agent auto-starts an ngrok tunnel if ngrok is installed:

```bash
# Automatic (recommended) — ngrok detected and started automatically
python provider.py

# Manual ngrok
ngrok http 11434
python provider.py --endpoint https://abc123.ngrok.io

# Tailscale (best for teams)
python provider.py --endpoint http://100.x.y.z:11434

# Port forward
python provider.py --endpoint http://your-public-ip:11434
```

## Pricing

The setup suggests pricing based on your GPU's actual throughput:

```
Estimated throughput: ~25 tokens/sec
Break-even price: $0.89/M tokens
Suggested range:  $1.78 - $4.44/M tokens

For comparison:
  OpenAI GPT-4o:       $5.00/M tokens
  Together AI:         $0.88/M tokens
  Your break-even:     $0.89/M tokens
```

Set price to $0 to share for free (recommended for building reputation).

Payment integration coming soon via purposebot.ai.

## All Options

```
python provider.py              # Interactive setup + run
python provider.py --install    # Install as permanent service
python provider.py --uninstall  # Remove service
python provider.py --status     # Check service status
python provider.py --config FILE # Use saved config (headless)
python provider.py --service    # Internal: headless mode for systemd/launchd
python provider.py --ollama URL # Ollama API URL (default: http://localhost:11434)
python provider.py --endpoint URL # Override public endpoint (skip ngrok)
python provider.py --no-ngrok  # Don't auto-start ngrok
```
