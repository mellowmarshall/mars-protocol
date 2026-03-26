# MARS GPU Provider Agent

Turn any machine with a GPU into a mesh inference provider in one command.

## Prerequisites

- [Ollama](https://ollama.com) installed and running
- A mesh gateway running (or connect to the public network)
- Python 3.9+ with `httpx`

## Quick Start

```bash
# Install Ollama (if you haven't already)
curl -fsSL https://ollama.com/install.sh | sh
ollama pull llama4

# Start a local gateway connected to the MARS network
./mesh-gateway --seed 5.161.53.251:4433 --listen 127.0.0.1:3000 &

# Share your GPU with the mesh
pip install httpx
python provider.py --gateway http://localhost:3000 --region us-east
```

That's it. The provider agent:

1. **Detects your GPU** (NVIDIA via nvidia-smi, AMD via rocm-smi)
2. **Lists installed Ollama models** (llama4, mistral, codellama, etc.)
3. **Publishes each model** as a mesh descriptor with hardware specs
4. **Re-publishes every 30 minutes** to keep listings alive

## What Gets Published

For each model on your GPU, a descriptor like this goes on the mesh:

```
type:     compute/inference/text-generation
endpoint: http://localhost:11434
params:
  name:              "llama4:latest (NVIDIA GeForce RTX 3090)"
  model:             "llama4:latest"
  gpu:               "NVIDIA GeForce RTX 3090"
  vram_mb:           24576
  price_per_mtok: 0.00
  region:            "us-east"
  ollama_api:        "http://localhost:11434/api/generate"
  openai_compat:     "http://localhost:11434/v1/chat/completions"
```

Other agents discover it with:
```python
providers = client.discover("compute/inference/text-generation")
```

## Options

```
--gateway URL     Mesh gateway URL (required)
--ollama URL      Ollama API URL (default: http://localhost:11434)
--endpoint URL    Public endpoint for this provider (default: Ollama URL)
--price FLOAT     Price per 1K tokens in USD (default: 0 = free)
--region NAME     Your region (e.g. us-east, eu-central)
--models LIST     Comma-separated models to advertise (default: all)
--refresh SECS    Re-publish interval (default: 1800)
```

## Making Your GPU Reachable

By default, Ollama only listens on localhost. For other agents to actually use your GPU:

**Option A: ngrok (easiest)**
```bash
ngrok http 11434
# Use the ngrok URL as --endpoint
python provider.py --gateway http://localhost:3000 --endpoint https://abc123.ngrok.io
```

**Option B: Tailscale (best for teams)**
```bash
# Ollama is reachable at your Tailscale IP
python provider.py --gateway http://localhost:3000 --endpoint http://100.x.y.z:11434
```

**Option C: Port forward (if you own the router)**
```bash
# Forward port 11434 to this machine
python provider.py --gateway http://localhost:3000 --endpoint http://your-public-ip:11434
```

## Pricing

Set `--price 0` to share for free (good for building reputation).
Set `--price 0.10` to charge $0.10 per 1K tokens.

Payment integration coming soon via purposebot.ai.
