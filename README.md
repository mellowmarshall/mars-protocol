<div align="center">

# mars-protocol

**Mesh Agent Routing Standard**

A decentralized capability discovery network for autonomous agents

[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/mellowmarshall/mars-protocol/blob/master/LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![Protocol Version](https://img.shields.io/badge/protocol-v0.1.0--draft-green.svg)](https://github.com/mellowmarshall/mars-protocol/blob/master/PROTOCOL.md)
[![PyPI](https://img.shields.io/pypi/v/mesh-protocol)](https://pypi.org/project/mesh-protocol/)
[![npm](https://img.shields.io/npm/v/mars-protocol)](https://www.npmjs.com/package/mars-protocol)
[![crates.io](https://img.shields.io/crates/v/mars-client)](https://crates.io/crates/mars-client)

<br>

*How does an AI agent find another agent that can review code, generate images, or search the web — without a central registry?*

**mars-protocol** is a [Kademlia DHT](https://en.wikipedia.org/wiki/Kademlia) over QUIC that lets machines publish, discover, and verify capabilities across a global peer-to-peer mesh. No API keys. No platform lock-in. No single point of failure.

<br>

[Wire Spec](https://github.com/mellowmarshall/mars-protocol/blob/master/PROTOCOL.md) · [Getting Started](https://github.com/mellowmarshall/mars-protocol/blob/master/docs/getting-started.md) · [Operator Guide](https://github.com/mellowmarshall/mars-protocol/blob/master/docs/operator-guide.md) · [Examples](https://github.com/mellowmarshall/mars-protocol/tree/master/examples)

</div>

---

## Live Network

The MARS mesh is live with 65+ services. Connect to any hub:

| Hub | Address | Location |
|-----|---------|----------|
| **us-east** | `5.161.53.251:4433` | Ashburn, VA |
| **us-west** | `5.78.197.92:4433` | Hillsboro, OR |
| **eu-central** | `46.225.55.16:4433` | Nuremberg, DE |
| **ap-southeast** | `5.223.69.128:4433` | Singapore |

---

## Quick Start — Pick Your Path

### Python (easiest)

```bash
pip install mesh-protocol
```

```python
from mesh_protocol import MeshClient

# Connect via a local gateway
with MeshClient("http://localhost:3000") as client:
    # Discover AI search providers
    providers = client.discover("data/search")
    for p in providers:
        print(f"{p.type} -> {p.endpoint}")

    # Discover LLM inference endpoints
    llms = client.discover("compute/inference/text-generation")

    # Publish your own capability
    client.publish(
        "compute/analysis/code-review",
        endpoint="https://my-agent.example.com/review",
        params={"languages": ["rust", "python"]},
    )
```

### TypeScript / JavaScript

```bash
npm install mars-protocol
```

```typescript
import { MeshClient } from "mars-protocol";

const client = new MeshClient("http://localhost:3000");
const providers = await client.discover("compute/inference");
await client.publish("compute/analysis/code-review", {
    endpoint: "https://my-agent.example.com/review",
});
```

### Rust (native, no gateway needed)

```bash
cargo add mars-client
```

```rust
let mut client = MeshClient::new(keypair, bind_addr).await?;
client.bootstrap(&[NodeAddr::quic("5.161.53.251:4433")]).await?;
client.publish_capability("compute/inference/text-generation",
    "https://my-agent.example.com/generate", None, &seed).await?;
let results = client.discover(&routing_key("compute/inference")).await?;
```

### HTTP (any language)

```bash
# Start the gateway
./mesh-gateway --seed 5.161.53.251:4433 --listen 0.0.0.0:3000

# Publish
curl -X POST http://localhost:3000/v1/publish \
  -H "Content-Type: application/json" \
  -d '{"type":"compute/inference/text-generation","endpoint":"https://..."}'

# Discover
curl http://localhost:3000/v1/discover?type=compute/inference
```

---

## Share Your GPU

Turn any machine with a GPU into a mesh inference provider:

```bash
# Install Ollama + pull a model
curl -fsSL https://ollama.com/install.sh | sh
ollama pull llama4

# Share your GPU (auto-detects hardware, starts ngrok tunnel, publishes to mesh)
pip install httpx
python tools/gpu-provider/provider.py --gateway http://localhost:3000
```

Other agents discover your GPU instantly:
```python
providers = client.discover("compute/inference/text-generation")
# → "llama4:latest (NVIDIA GeForce RTX 3090)" — free, us-east
```

See [GPU Provider docs](https://github.com/mellowmarshall/mars-protocol/tree/master/tools/gpu-provider) for pricing, regions, and networking options.

---

## Bridges

Connect existing ecosystems to the mesh:

| Bridge | Install | What it does |
|--------|---------|-------------|
| **[MCP Bridge](https://github.com/mellowmarshall/mars-protocol/tree/master/bridges/mcp)** | `pip install mesh-mcp-bridge` | Publish MCP server tools → mesh. Discover mesh capabilities as MCP tools. |
| **[OpenAPI Bridge](https://github.com/mellowmarshall/mars-protocol/tree/master/bridges/openapi)** | `pip install httpx pyyaml` | Point at any Swagger/OpenAPI spec → register all endpoints on mesh. |

```bash
# Publish an MCP server's tools to the mesh
mesh-mcp-bridge publish --gateway http://localhost:3000 --mcp-server "python my_server.py"

# Register an entire REST API from its OpenAPI spec
python bridges/openapi/openapi_bridge.py --gateway http://localhost:3000 \
    --spec https://petstore.swagger.io/v2/swagger.json
```

---

## Why

Every AI agent framework reinvents service discovery. MCP servers need manual configuration. Tool registries are centralized bottlenecks. When you have thousands of agents across multiple organizations, "just add it to the config file" doesn't scale.

**mars-protocol** makes capability discovery a network primitive:

```
┌──────────────┐         ┌──────────────┐         ┌──────────────┐
│  Agent A     │         │  Mesh Hub    │         │  Agent B     │
│              │ STORE   │              │ FIND    │              │
│  "I can do   │────────▶│  Kademlia    │◀────────│  "Who can    │
│   code       │         │  DHT over    │         │   review     │
│   review"    │         │  QUIC        │         │   code?"     │
└──────────────┘         └──────────────┘         └──────────────┘
```

---

## Architecture

<table>
<tr>
<td width="50%">

### Crate Map

| Crate | Description |
|-------|-------------|
| **mesh-core** | Wire types, CBOR serialization, Ed25519 identity, content-addressed hashing |
| **mesh-transport** | QUIC transport with mutual TLS (Ed25519 self-signed certs) |
| **mesh-dht** | Kademlia DHT — routing table, iterative lookup, descriptor storage |
| **mesh-schemas** | Well-known schema hashes and routing key constants |
| **mars-client** | High-level client library (`MeshClient` — bootstrap, publish, discover, ping) |
| **mesh-node** | CLI binary — run a mesh node, publish/discover capabilities |
| **mesh-hub** | Production hub — redb storage, multi-tenant, admin API, peering, metrics |
| **mesh-gateway** | HTTP/JSON gateway — lets any language use the mesh |

</td>
<td width="50%">

### Protocol Stack

```
┌─────────────────────────────┐
│  Application Layer          │
│  Descriptors + Schemas      │
├─────────────────────────────┤
│  DHT Layer                  │
│  Kademlia (STORE/FIND_VALUE │
│  /FIND_NODE/PING)           │
├─────────────────────────────┤
│  Security Layer             │
│  Mutual TLS + Ed25519       │
│  Sender-TLS Binding         │
├─────────────────────────────┤
│  Transport Layer            │
│  QUIC (RFC 9000)            │
└─────────────────────────────┘
```

</td>
</tr>
</table>

### SDKs & Bridges

| Package | Language | Install | Docs |
|---------|----------|---------|------|
| **mars-client** | Rust | `cargo add mars-client` | [crates.io](https://crates.io/crates/mars-client) |
| **mesh-protocol** | Python | `pip install mesh-protocol` | [PyPI](https://pypi.org/project/mesh-protocol/) |
| **mars-protocol** | TypeScript | `npm install mars-protocol` | [npm](https://www.npmjs.com/package/mars-protocol) |
| **mesh-mcp-bridge** | Python | `pip install mesh-mcp-bridge` | [PyPI](https://pypi.org/project/mesh-mcp-bridge/) |
| **openapi-bridge** | Python | `pip install httpx pyyaml` | [README](https://github.com/mellowmarshall/mars-protocol/tree/master/bridges/openapi) |

---

## What Agents Publish

A **descriptor** is a signed, content-addressed record that says "I can do X, reach me at Y":

```
Descriptor {
    publisher:    did:mesh:z6Mkt...     # Ed25519 identity (self-certifying)
    schema_hash:  blake3(core/capability)
    topic:        "compute/inference/text-generation"
    routing_keys: [blake3("compute"), blake3("compute/inference"), blake3("compute/inference/text-generation")]
    payload:      { endpoint: "https://...", model: "llama-4-scout", max_tokens: 4096 }
    signature:    Ed25519(publisher, canonical_cbor(descriptor))
    ttl:          3600
    sequence:     1          # monotonic — newer replaces older
}
```

Routing keys are **hierarchical** — searching for `compute/inference` finds all inference providers (text, image, speech, embeddings), while `compute/inference/text-generation` narrows to exactly that.

---

## Running a Hub

```bash
# Generate hub identity
./mesh-hub --generate-keypair

# Edit config (see examples/hub.toml)
cp examples/hub.toml mesh-hub.toml
$EDITOR mesh-hub.toml

# Run
./mesh-hub --config mesh-hub.toml
```

Hub features: disk-backed storage (redb), multi-tenant with MU metering, admin API, hub-to-hub peering via gossip, Prometheus metrics, rate limiting, SSRF protection.

See the [Operator Guide](https://github.com/mellowmarshall/mars-protocol/blob/master/docs/operator-guide.md) for full configuration reference. Multi-region deployment scripts in [`deploy/`](https://github.com/mellowmarshall/mars-protocol/tree/master/deploy).

---

## Security Model

<table>
<tr>
<td>

### Hybrid Authentication
- **Descriptors** carry Ed25519 publisher signatures — self-authenticating across relays, caches, and federation
- **Protocol messages** use mutual TLS with Ed25519 identity binding — zero per-message overhead

### Defense in Depth
- Sender-TLS binding on every protocol message
- Sybil-resistant routing (LRU ping challenge on full K-buckets)
- Descriptor revocation and key rotation
- SSRF prevention (blocks RFC1918, CGN, ULA, loopback)
- Per-IP and per-identity rate limiting

</td>
<td>

### Cryptographic Guarantees
- **Identity**: Ed25519 keypairs, DID-based identifiers
- **Hashing**: BLAKE3 with algorithm agility (version byte)
- **Transport**: QUIC with mutual TLS, mandatory client auth
- **Serialization**: RFC 8949 deterministic CBOR
- **Descriptors**: Self-signed, sequence-monotonic, TTL-bounded

</td>
</tr>
</table>

---

## Use Cases

<table>
<tr>
<td width="33%" valign="top">

### Agent-to-Agent Discovery
An AI agent needs code review. It queries the mesh for `compute/analysis/code-review`, gets back signed descriptors, verifies identity, and invokes the best match.

</td>
<td width="33%" valign="top">

### Distributed GPU Inference
GPU owners share their hardware on the mesh. Agents discover the cheapest/fastest provider for their workload — no marketplace middleman.

</td>
<td width="33%" valign="top">

### Federated Tool Registry
Publish MCP server tools to the mesh. Agents discover tools dynamically — no config files, no central registry, no single point of failure.

</td>
</tr>
</table>

---

## Contributing

```bash
cargo test --workspace    # Run the full test suite (230+ tests)
cargo test --package mesh-hub   # Hub hardening tests only
```

---

## License

Dual-licensed under [MIT](https://github.com/mellowmarshall/mars-protocol/blob/master/LICENSE-MIT) or [Apache-2.0](https://github.com/mellowmarshall/mars-protocol/blob/master/LICENSE-APACHE) at your option.
