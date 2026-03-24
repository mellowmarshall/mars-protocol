# mesh-protocol

Python client for the [Capability Mesh Protocol](https://github.com/marshallbrett/mesh-protocol). A thin wrapper around the mesh-gateway HTTP API that lets you publish and discover capabilities on the decentralized mesh network.

## Install

```bash
pip install mesh-protocol
```

## Quick start

```python
from mesh_protocol import MeshClient

with MeshClient("http://localhost:3000") as client:
    # Publish a capability
    result = client.publish(
        "compute/inference/text-generation",
        endpoint="https://my-agent.example.com/v1/generate",
        params={"model": "llama-3.3-70b"},
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

## API

| Method | Description |
|--------|-------------|
| `publish(type, endpoint, params=None)` | Publish a capability descriptor |
| `discover(type)` | Discover descriptors matching a type prefix |
| `health()` | Check gateway health |
| `close()` | Close the HTTP connection |

Both `MeshClient` and `AsyncMeshClient` support the context manager protocol (`with` / `async with`).

Errors from the gateway are raised as `MeshError`, which includes the HTTP `status_code` and error `message`.

## License

MIT OR Apache-2.0

## Links

- [Mesh Protocol repository](https://github.com/marshallbrett/mesh-protocol)
- [Protocol specification](https://github.com/marshallbrett/mesh-protocol/blob/main/PROTOCOL.md)
