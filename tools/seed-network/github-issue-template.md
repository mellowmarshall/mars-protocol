# GitHub Issue Template — "Your API is discoverable on MARS"

> Copy and customize for each repo. Replace {COMPANY}, {TYPE}, {ENDPOINT}.

---

**Title:** Your API is now discoverable on the MARS mesh network

**Body:**

Hey team 👋

I registered {COMPANY}'s API as a capability on the [MARS mesh network](https://github.com/mellowmarshall/mars-protocol) — a decentralized discovery layer for AI agents.

This means any agent on the mesh can now discover your service automatically:

```python
pip install mesh-protocol
```

```python
from mesh_protocol import MeshClient

client = MeshClient("http://localhost:3000")  # via mesh gateway
providers = client.discover("{TYPE}")

for p in providers:
    print(f"{p.type} -> {p.endpoint}")
    # {TYPE} -> {ENDPOINT}
```

**What this means for you:**
- AI agents using MARS can find {COMPANY} without any manual configuration
- Zero cost, zero maintenance — the descriptor is already published
- If you'd like to manage your own listing, you can publish directly via the gateway API or the Python SDK

**What is MARS?**
MARS (Mesh Agent Routing Standard) is a Kademlia DHT over QUIC for decentralized capability discovery. Think "DNS for AI agents" — agents publish what they can do, other agents discover them, no central registry.

- GitHub: https://github.com/mellowmarshall/mars-protocol
- Live network: 4 hubs across US, EU, and Singapore
- SDKs: Python (`pip install mesh-protocol`), TypeScript (`npm install mars-protocol`), Rust (`cargo add mars-client`)

Happy to answer any questions. If you want to publish additional capabilities or update the listing, the [Python SDK docs](https://pypi.org/project/mesh-protocol/) have everything you need.

---

> **Repos to open issues on (high value, have GitHub repos):**
>
> - https://github.com/mendableai/firecrawl — Firecrawl (web scraping)
> - https://github.com/tavily-ai/tavily-python — Tavily (AI search)
> - https://github.com/exa-labs/exa-py — Exa (neural search)
> - https://github.com/e2b-dev/e2b — E2B (code sandboxes)
> - https://github.com/langfuse/langfuse — Langfuse (LLM observability)
> - https://github.com/resend/resend-python — Resend (email API)
> - https://github.com/upstash/upstash-redis — Upstash (serverless Redis)
