# mars-protocol

Python client for the [Mesh Agent Routing Standard](https://github.com/mellowmarshall/mars-protocol). A thin wrapper around the mesh-gateway HTTP API that lets you publish and discover capabilities on the decentralized mesh network.

## Install

```bash
pip install mesh-protocol
```

## Live Network

The MARS mesh is live. Start a gateway connected to any hub, then use this SDK:

```bash
# Start the gateway (connects to mesh via QUIC, exposes HTTP)
./target/release/mesh-gateway --seed 5.161.53.251:4433 --listen 0.0.0.0:3000
```

Available hubs:

| Hub | Address | Location |
|-----|---------|----------|
| **us-east** | `5.161.53.251:4433` | Ashburn, VA |
| **us-west** | `5.78.197.92:4433` | Hillsboro, OR |
| **eu-central** | `46.225.55.16:4433` | Nuremberg, DE |
| **ap-southeast** | `5.223.69.128:4433` | Singapore |

## Quick start

```python
from mesh_protocol import MeshClient

with MeshClient("http://localhost:3000") as client:
    # Publish a capability
    result = client.publish(
        "compute/inference/text-generation",
        endpoint="https://my-agent.example.com/v1/generate",
        params={"model": "glm-5"},
    )
    print(result.descriptor_id)

    # Discover capabilities
    providers = client.discover("compute/inference")
    for p in providers:
        print(f"{p.type} -> {p.endpoint}")
```

## Async

```python
import asyncio
from mesh_protocol import AsyncMeshClient

async def main():
    async with AsyncMeshClient("http://localhost:3000") as client:
        await client.publish(
            "storage/blob",
            endpoint="https://my-storage.example.com/v1/upload",
        )
        providers = await client.discover("storage")
        print(providers)

asyncio.run(main())
```

## Keeping services alive

Descriptors have a TTL (default 1 hour). Use `publish_maintained()` for long-running services — it re-publishes automatically in a background thread:

```python
from mesh_protocol import MeshClient

with MeshClient("http://localhost:3000") as client:
    with client.publish_maintained(
        "compute/inference/text-generation",
        endpoint="https://my-agent.example.com/v1/generate",
    ) as desc:
        print(f"Published: {desc.descriptor_id}")
        # Descriptor stays alive as long as this block runs
        while True:
            time.sleep(60)
```

You can also set a custom TTL (up to 24 hours):

```python
client.publish("compute/analysis/research", endpoint="...", ttl=86400)
```

## API

| Method | Description |
|--------|-------------|
| `publish(type, endpoint, params=None, ttl=None)` | Publish a capability descriptor |
| `publish_maintained(type, endpoint, params=None, refresh_interval=1800)` | Publish and auto-refresh in background |
| `discover(type)` | Discover descriptors matching a type prefix |
| `health()` | Check gateway health |
| `close()` | Close the HTTP connection |

Both `MeshClient` and `AsyncMeshClient` support the context manager protocol (`with` / `async with`).

`publish_maintained()` returns a `MaintainedDescriptor` (or `AsyncMaintainedDescriptor`) with `.stop()`, `.is_alive()`, and context manager support.

Errors from the gateway are raised as `MeshError`, which includes the HTTP `status_code` and error `message`.

## License

MIT OR Apache-2.0

## Links

- [MARS Protocol repository](https://github.com/mellowmarshall/mars-protocol)
- [Protocol specification](https://github.com/mellowmarshall/mars-protocol/blob/master/PROTOCOL.md)
