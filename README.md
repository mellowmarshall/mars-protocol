<div align="center">

# mesh-protocol

**A decentralized capability discovery network for autonomous agents**

[![License: MIT/Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![Protocol Version](https://img.shields.io/badge/protocol-v0.1.0--draft-green.svg)](PROTOCOL.md)

<br>

*How does an AI agent find another agent that can review code, generate images, or search the web — without a central registry?*

**mesh-protocol** is a [Kademlia DHT](https://en.wikipedia.org/wiki/Kademlia) over QUIC that lets machines publish, discover, and verify capabilities across a global peer-to-peer mesh. No API keys. No platform lock-in. No single point of failure.

<br>

[Wire Spec](PROTOCOL.md) · [Getting Started](docs/getting-started.md) · [Operator Guide](docs/operator-guide.md) · [Examples](examples/)

</div>

---

## Why

Every AI agent framework reinvents service discovery. MCP servers need manual configuration. Tool registries are centralized bottlenecks. When you have thousands of agents across multiple organizations, "just add it to the config file" doesn't scale.

**mesh-protocol** makes capability discovery a network primitive:

```
┌──────────────┐         ┌──────────────┐         ┌──────────────┐
│  Agent A     │         │  Mesh Hub    │         │  Agent B     │
│              │ STORE   │              │ FIND    │              │
│  "I can do   │────────▶│  Kademlia    │◀────────│  "Who can    │
│   code       │         │  DHT over    │         │   review     │
│   review"    │         │  QUIC        │         │   code?"     │
└──────────────┘         └──────────────┘         └──────────────┘
```

- **Publish** a signed descriptor saying what you can do
- **Discover** capabilities by type, with hierarchical routing (`compute/inference/text-generation`)
- **Verify** that a descriptor was actually published by who it claims — Ed25519 signatures, no trust required

## Design Principles

> *This protocol is designed to outlast its creators.*

1. **No opinions about capabilities.** The mesh routes content-addressed descriptors. What they mean is up to you.
2. **The core never changes.** Evolution happens in payload schemas.
3. **Every identifier is a content-hash.** No registries, no DNS, no authorities.
4. **Cryptographic identity everywhere.** Descriptors are self-authenticating. Transport uses mutual TLS.
5. **Algorithm agility.** No hardcoded cryptography. Hash algorithms are versioned.
6. **No global state.** Eventually consistent. Scales to billions.

---

## Quick Start

```bash
# Build everything
cargo build --release

# Generate an identity
./target/release/mesh-node identity --generate --path my-agent.key

# Start a node (or connect to an existing hub)
./target/release/mesh-node start --listen 0.0.0.0:4433 --identity my-agent.key

# Publish a capability
./target/release/mesh-node publish \
    --type "compute/inference/text-generation" \
    --endpoint "https://my-agent.example.com/v1/generate" \
    --params '{"model":"llama-3.3-70b","max_tokens":4096}' \
    --seed 127.0.0.1:4433 \
    --identity my-agent.key

# Discover capabilities
./target/release/mesh-node discover \
    --type "compute/inference" \
    --seed 127.0.0.1:4433 \
    --identity my-agent.key
```

See the [Getting Started Guide](docs/getting-started.md) for multi-node setup and hub deployment.

### For non-Rust agents (Python, TypeScript, Go, etc.)

Run the HTTP gateway and use simple REST calls:

```bash
# Start the gateway (connects to mesh via QUIC, exposes HTTP)
./target/release/mesh-gateway --seed 127.0.0.1:4433 --listen 0.0.0.0:3000
```

```python
# Python agent publishes a capability
requests.post("http://localhost:3000/v1/publish", json={
    "type": "compute/inference/text-generation",
    "endpoint": "https://my-agent.example.com/v1/generate",
    "params": {"model": "llama-3.3-70b"}
})

# Another agent discovers it
r = requests.get("http://localhost:3000/v1/discover?type=compute/inference")
print(r.json()["descriptors"])
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
| **mesh-client** | High-level client library (`MeshClient` — bootstrap, publish, discover, ping) |
| **mesh-node** | CLI binary — run a mesh node, publish/discover capabilities |
| **mesh-hub** | Production hub — redb storage, multi-tenant, admin API, peering, metrics |
| **mesh-gateway** | HTTP/JSON gateway — lets Python, TypeScript, Go agents use the mesh |

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

---

## What Agents Publish

A **descriptor** is a signed, content-addressed record that says "I can do X, reach me at Y":

```
Descriptor {
    publisher:    did:mesh:z6Mkt...     # Ed25519 identity (self-certifying)
    schema_hash:  blake3(core/capability)
    topic:        "compute/inference/text-generation"
    routing_keys: [blake3("compute"), blake3("compute/inference"), blake3("compute/inference/text-generation")]
    payload:      CBOR { endpoint: "https://...", model: "llama-3.3-70b", max_tokens: 4096 }
    signature:    Ed25519(publisher, canonical_cbor(descriptor))
    ttl:          3600
    sequence:     1          # monotonic — newer replaces older
}
```

Routing keys are **hierarchical** — searching for `compute/inference` finds all inference providers (text, image, speech, embeddings), while `compute/inference/text-generation` narrows to exactly that.

---

## Running a Hub

A hub is a high-capacity mesh node for production deployments:

```bash
# Generate hub identity
./target/release/mesh-hub --generate-keypair

# Edit config (see examples/hub.toml)
cp examples/hub.toml mesh-hub.toml
$EDITOR mesh-hub.toml

# Run
./target/release/mesh-hub --config mesh-hub.toml
```

Hub features:
- **Disk-backed storage** (redb) with LRU hot cache
- **Multi-tenant** — per-org quotas, MU metering, DID-Auth challenge-response
- **Admin API** — tenant management, metrics, health checks
- **Hub peering** — federate with other hubs via gossip
- **Prometheus metrics** — aggregate-only (no tenant data leakage)
- **Rate limiting** — per-IP and per-identity sliding window
- **SSRF protection** — blocks outbound connections to private address ranges

See the [Operator Guide](docs/operator-guide.md) for full configuration reference.

---

## Security Model

<table>
<tr>
<td>

### Hybrid Authentication
- **Descriptors** carry Ed25519 publisher signatures — self-authenticating across relays, caches, and federation boundaries
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
- **Serialization**: RFC 8949 deterministic CBOR for content hashing
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
An [OpenClaw](https://github.com/openclaw) agent needs code review. It queries the mesh for `compute/analysis/code-review`, gets back signed descriptors from agents that offer that capability, verifies their identity, and invokes the one with the best fit.

</td>
<td width="33%" valign="top">

### Federated Tool Registry
Instead of manually configuring MCP servers in every agent, publish tool capabilities to the mesh. Agents discover tools dynamically — no config files, no central registry, no single point of failure.

</td>
<td width="33%" valign="top">

### IoT + Edge Compute
Sensors publish `data/sensor/temperature` descriptors. Edge compute nodes publish `compute/analysis/anomaly-detection`. The mesh connects producers to consumers without a cloud broker.

</td>
</tr>
</table>

---

## Examples

| Example | Description |
|---------|-------------|
| [`examples/hub.toml`](examples/hub.toml) | Production hub configuration with all options documented |
| [`examples/agent-publish.sh`](examples/agent-publish.sh) | AI agent publishing capabilities (inference, code review, search) |
| [`examples/agent-discover.sh`](examples/agent-discover.sh) | AI agent discovering capabilities with hierarchical routing |
| [`examples/node-relay.sh`](examples/node-relay.sh) | Relay node setup to strengthen the mesh |

---

## Project Status

mesh-protocol is **feature-complete for v0.1** and ready for deployment.

| Component | Status |
|-----------|--------|
| Wire protocol (PROTOCOL.md) | Stable draft |
| Core types + CBOR + identity | Complete |
| QUIC transport + mutual TLS | Complete |
| Kademlia DHT | Complete |
| mesh-node CLI | Complete |
| mesh-client library | Complete |
| mesh-hub (production) | Complete |
| Multi-tenant + metering | Complete |
| Hub peering + gossip | Complete |
| Observability + metrics | Complete |
| Security hardening | Complete (15 integration tests) |
| Documentation | Complete |

**Deferred:** Voucher system (prepaid credits), holder-DID binding (Security #9).

---

## Contributing

```bash
# Run the full test suite
cargo test --workspace

# Run just the hub hardening tests
cargo test --package mesh-hub

# Check the wire spec
cat PROTOCOL.md
```

---

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at your option.
