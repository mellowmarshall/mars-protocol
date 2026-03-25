# MARS ↔ Petals Bridge

Connect the [Petals](https://petals.dev) distributed inference swarm to the MARS mesh network. Models running across hundreds of volunteer GPUs become discoverable by any agent.

## How It Works

```
MARS Mesh                    Bridge                     Petals Swarm
┌─────────┐                ┌─────────┐                ┌─────────────┐
│ Agents   │◀── discover ──│ Monitors│── health API ──▶│ 100+ GPUs   │
│ discover │               │ Petals  │   or DHT        │ serving     │
│ "compute/│               │ swarm,  │                 │ Llama 405B, │
│ inference│               │ publishes│                │ Mixtral,    │
│ "        │               │ to MARS │                 │ Falcon...   │
└─────────┘                └─────────┘                └─────────────┘
```

Petals splits large models (70B–405B parameters) across many consumer GPUs using model parallelism over the internet. This bridge publishes those models as MARS descriptors so any agent can discover and use them.

## Usage

```bash
pip install httpx

# Basic — discover via health API and publish to MARS
python petals_bridge.py --gateway http://localhost:3000 --health-api

# Direct DHT connection (requires: pip install petals)
python petals_bridge.py --gateway http://localhost:3000

# Run once (no refresh loop)
python petals_bridge.py --gateway http://localhost:3000 --health-api --once

# Preview without publishing
python petals_bridge.py --dry-run --health-api

# Refresh every 5 minutes (default)
python petals_bridge.py --gateway http://localhost:3000 --health-api
```

## What Gets Published

For each active model on the Petals swarm:

```
type:     compute/inference/text-generation
endpoint: https://chat.petals.dev/api/v1/chat/completions
params:
  name:              "Meta-Llama-3.1-405B-Instruct (Petals Swarm)"
  model:             "meta-llama/Meta-Llama-3.1-405B-Instruct"
  provider:          "petals"
  active_servers:    47
  distributed:       true
  openai_compatible: true
  auth:              "none"
```

Agents discover it with:
```python
providers = client.discover("compute/inference/text-generation")
# → "Meta-Llama-3.1-405B-Instruct (Petals Swarm)" — 47 servers, free, OpenAI-compatible
```

## Using Petals Models

The endpoint is OpenAI-compatible. Any agent can call it directly:

```python
import httpx

r = httpx.post("https://chat.petals.dev/api/v1/chat/completions", json={
    "model": "meta-llama/Meta-Llama-3.1-405B-Instruct",
    "messages": [{"role": "user", "content": "Hello!"}],
    "max_tokens": 100,
})
print(r.json()["choices"][0]["message"]["content"])
```

No API key required. Free. Distributed across volunteer GPUs worldwide.

## Links

- [Petals](https://petals.dev) — Run LLMs at home, BitTorrent-style
- [Petals GitHub](https://github.com/bigscience-workshop/petals)
- [MARS Protocol](https://github.com/mellowmarshall/mars-protocol)
