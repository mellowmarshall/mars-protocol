# Getting Started with Mesh Protocol

## Prerequisites

- **Rust toolchain** (1.75+ recommended): install via [rustup](https://rustup.rs/)
- cargo, included with rustup

## Build

```bash
git clone <repo-url> && cd mesh-protocol
cargo build --release
```

Binaries:
- `target/release/mesh-node` -- lightweight mesh node and CLI
- `target/release/mesh-hub` -- high-capacity hub node

## Generate Identity

Every node needs an Ed25519 keypair. Generate one:

```bash
./target/release/mesh-node identity --generate --path ~/.mesh/node.key
```

Output:
```
Generated new identity:
  DID: did:mesh:z6Mk...
  Saved to: /home/you/.mesh/node.key
```

To inspect an existing key:

```bash
./target/release/mesh-node identity --path ~/.mesh/node.key
```

## Run a Node

Start a mesh node listening for QUIC connections:

```bash
./target/release/mesh-node start \
  --listen 0.0.0.0:4433 \
  --seed 198.51.100.1:4433 \
  --identity ~/.mesh/node.key
```

- `--listen` -- bind address (default: `0.0.0.0:4433`)
- `--seed` -- address of an existing node to bootstrap from (repeatable)
- `--identity` -- path to keypair file (generates ephemeral if omitted)

The node bootstraps its Kademlia routing table from the seed, then listens for PING, STORE, FIND_NODE, and FIND_VALUE messages.

## Publish a Capability

Publish a capability descriptor to the mesh:

```bash
./target/release/mesh-node publish \
  --type compute/inference/text-generation \
  --endpoint 203.0.113.10:5000 \
  --params '{"model": "llama-3", "max_tokens": 4096}' \
  --seed 198.51.100.1:4433 \
  --identity ~/.mesh/node.key
```

- `--type` -- hierarchical capability type (used for routing key computation)
- `--endpoint` -- provider's service address
- `--params` -- optional JSON metadata
- `--seed` -- target node to STORE the descriptor on

The descriptor is signed by the node's identity, hashed, and stored in the DHT under hierarchical routing keys (e.g., `compute`, `compute/inference`, `compute/inference/text-generation`).

## Discover Capabilities

Search the mesh for capabilities by type:

```bash
./target/release/mesh-node discover \
  --type compute/inference \
  --seed 198.51.100.1:4433 \
  --identity ~/.mesh/node.key
```

This performs an iterative Kademlia `lookup_value` for the routing key derived from the capability type. Results include all matching descriptors:

```
Discovering: compute/inference
  Routing key: <hash>
  Bootstrap: discovered 3 nodes
  Found 1 descriptor(s):
    ---
    Publisher: did:mesh:z6Mk...
    Topic:     compute/inference/text-generation
    ID:        <descriptor-id>
    Payload:   {"endpoint": "203.0.113.10:5000", ...}
```

## Ping a Node

Check connectivity to a remote node:

```bash
./target/release/mesh-node ping \
  --addr 198.51.100.1:4433 \
  --identity ~/.mesh/node.key
```

Returns the peer's DID, its observed address for you, and its own address.

## Run a Hub

Hubs are high-capacity nodes with disk-backed storage, multi-tenant management, and an admin API. See the [Operator Guide](operator-guide.md) for full configuration reference.

### 1. Create a config file

```toml
# mesh-hub.toml
[identity]
keypair_path = "data/hub.key"

[network]
listen_addr = "0.0.0.0:4433"
admin_addr = "127.0.0.1:8080"

operator_token = "change-me-in-production"

[policy]
store_mode = "open"
```

### 2. Generate the hub keypair

```bash
./target/release/mesh-hub --config mesh-hub.toml --generate-keypair
```

### 3. Start the hub

```bash
./target/release/mesh-hub --config mesh-hub.toml
```

The hub starts three concurrent subsystems:
- **QUIC protocol listener** on `listen_addr` -- handles PING, STORE, FIND_NODE, FIND_VALUE
- **Admin HTTP API** on `admin_addr` -- tenant management, health checks, metrics
- **Background tasks** -- descriptor expiry (60s), rate limiter cleanup (120s)

### 4. Verify

```bash
curl http://localhost:8080/healthz        # 200 OK
curl http://localhost:8080/readyz         # 200 OK
curl http://localhost:8080/api/v1/hub/status  # JSON status
```

## Multi-Node Setup

### Seed Nodes

In a mesh network, nodes bootstrap their routing tables from seed nodes. A seed node is just a regular node (or hub) that is already running and reachable.

Start the first node (it has no seeds -- it is the seed):

```bash
mesh-node start --listen 0.0.0.0:4433 --identity ~/.mesh/seed.key
```

Start additional nodes pointing to the seed:

```bash
mesh-node start --listen 0.0.0.0:4434 \
  --seed 198.51.100.1:4433 \
  --identity ~/.mesh/node2.key
```

Nodes can specify multiple `--seed` flags for redundancy.

### Hub Peering

For multi-hub deployments, enable peering so hubs replicate descriptors via gossip:

```toml
# hub-a.toml
[peering]
enabled = true
gossip_interval_secs = 30
max_peers = 10
regions = ["us-east"]

[security]
outbound_allowlist = ["10.0.1.6:4433"]
```

```toml
# hub-b.toml
[peering]
enabled = true
gossip_interval_secs = 30
max_peers = 10
regions = ["us-west"]

[security]
outbound_allowlist = ["10.0.1.5:4433"]
```

Hubs discover each other automatically through DHT advertisements. Each hub publishes a self-advertisement descriptor under the `infrastructure/hub` routing key. Other hubs find these advertisements and establish peer connections.

For hubs on private networks, add peer addresses to `security.outbound_allowlist` to bypass the SSRF filter.

## Using the Client Library

The `mesh-client` crate provides a high-level Rust API:

```rust
use mesh_client::MeshClient;
use mesh_core::identity::Keypair;
use mesh_core::message::NodeAddr;
use mesh_core::routing::routing_key;
use mesh_core::{Descriptor};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let keypair = Keypair::generate();
    let bind_addr = "0.0.0.0:0".parse()?;

    let mut client = MeshClient::new(keypair, bind_addr).await?;

    // Bootstrap from a seed node
    let seed = NodeAddr {
        protocol: "quic".into(),
        address: "198.51.100.1:4433".into(),
    };
    let discovered = client.bootstrap(&[seed.clone()]).await?;
    println!("Discovered {discovered} nodes");

    // Publish a descriptor
    // (build your Descriptor with Descriptor::create, then:)
    // let ack = client.publish(descriptor, &seed).await?;

    // Discover by routing key
    let rk = routing_key("compute/inference");
    let descriptors = client.discover(&rk).await?;
    for desc in &descriptors {
        println!("Found: {} by {}", desc.topic, desc.publisher.did());
    }

    // Ping a node
    let pong = client.ping(&seed).await?;
    println!("Pong from {}", pong.sender.did());

    Ok(())
}
```

Add to your `Cargo.toml`:

```toml
[dependencies]
mesh-client = { path = "../mesh-client" }
mesh-core = { path = "../mesh-core" }
tokio = { version = "1", features = ["full"] }
```

### Key Types

- **`MeshClient`** -- wraps identity, QUIC transport, and DHT node
- **`Descriptor`** -- a signed capability record with schema, topic, payload, routing keys, and TTL
- **`NodeAddr`** -- protocol + address pair (e.g., `quic://198.51.100.1:4433`)
- **`Hash`** -- 32-byte routing key for DHT lookups
- **`Keypair` / `Identity`** -- Ed25519 keypair and its derived DID (`did:mesh:z6Mk...`)
