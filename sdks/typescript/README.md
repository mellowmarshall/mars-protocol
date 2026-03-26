# mars-protocol

TypeScript client for the [Mesh Agent Routing Standard](https://github.com/mellowmarshall/mars-protocol). A thin wrapper around the mesh-gateway HTTP API for publishing and discovering capabilities on the decentralized mesh network.

## Install

```bash
npm install mars-protocol
```

## Quick Start

```ts
import { MeshClient } from "mars-protocol";

const client = new MeshClient("http://localhost:3000");

// Publish a capability
const result = await client.publish("compute/inference/text-generation", {
  endpoint: "https://my-agent.example.com/v1/generate",
  params: { model: "glm-5", max_tokens: 4096 },
});
console.log(result.descriptor_id);

// Discover capabilities
const descriptors = await client.discover("compute/inference");
for (const d of descriptors) {
  console.log(`${d.type} -> ${d.endpoint} (${d.publisher})`);
}

// Health check
const health = await client.health();
console.log(health.identity);
```

## Gateway Setup

The client connects to a [mesh-gateway](https://github.com/mellowmarshall/mars-protocol) instance:

```bash
# Start a gateway connected to the live MARS network
./mesh-gateway --seed 5.161.53.251:4433 --listen 0.0.0.0:3000
```

## Live Network

| Hub | Address |
|-----|---------|
| us-east | `5.161.53.251:4433` |
| us-west | `5.78.197.92:4433` |
| eu-central | `46.225.55.16:4433` |
| ap-southeast | `5.223.69.128:4433` |

## Links

- [MARS Protocol repository](https://github.com/mellowmarshall/mars-protocol)
- [Protocol specification](https://github.com/mellowmarshall/mars-protocol/blob/master/PROTOCOL.md)
