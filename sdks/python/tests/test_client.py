"""Tests for the Mesh Protocol Python client."""

from __future__ import annotations

import httpx
import pytest
import respx

from mesh_protocol import (
    AsyncMeshClient,
    Descriptor,
    HealthStatus,
    MeshClient,
    MeshError,
    PublishResult,
)

GATEWAY = "http://localhost:3000"

# ── Fixtures ────────────────────────────────────────────────────────────

SAMPLE_DESCRIPTOR = {
    "id": "blake3:abc123",
    "publisher": "did:key:z6Mktest",
    "type": "compute/inference/text-generation",
    "endpoint": "https://agent.example.com/v1/generate",
    "params": {"model": "llama-3.3-70b"},
    "timestamp": 1700000000000000,
    "ttl": 3600,
    "sequence": 1,
}


# ── Synchronous tests ──────────────────────────────────────────────────


@respx.mock
def test_publish():
    route = respx.post(f"{GATEWAY}/v1/publish").mock(
        return_value=httpx.Response(
            200,
            json={"ok": True, "descriptor_id": "blake3:abc123"},
        )
    )

    client = MeshClient(GATEWAY)
    result = client.publish(
        "compute/inference/text-generation",
        endpoint="https://agent.example.com/v1/generate",
        params={"model": "llama-3.3-70b"},
    )
    client.close()

    assert isinstance(result, PublishResult)
    assert result.ok is True
    assert result.descriptor_id == "blake3:abc123"

    # Verify the request body sent to the gateway.
    request = route.calls[0].request
    body = request.content
    import json

    parsed = json.loads(body)
    assert parsed["type"] == "compute/inference/text-generation"
    assert parsed["endpoint"] == "https://agent.example.com/v1/generate"
    assert parsed["params"] == {"model": "llama-3.3-70b"}


@respx.mock
def test_discover():
    respx.get(f"{GATEWAY}/v1/discover").mock(
        return_value=httpx.Response(
            200,
            json={"descriptors": [SAMPLE_DESCRIPTOR]},
        )
    )

    client = MeshClient(GATEWAY)
    results = client.discover("compute/inference")
    client.close()

    assert len(results) == 1
    d = results[0]
    assert isinstance(d, Descriptor)
    assert d.id == "blake3:abc123"
    assert d.publisher == "did:key:z6Mktest"
    assert d.type == "compute/inference/text-generation"
    assert d.endpoint == "https://agent.example.com/v1/generate"
    assert d.params == {"model": "llama-3.3-70b"}
    assert d.timestamp == 1700000000000000
    assert d.ttl == 3600
    assert d.sequence == 1


@respx.mock
def test_discover_empty():
    respx.get(f"{GATEWAY}/v1/discover").mock(
        return_value=httpx.Response(
            200,
            json={"descriptors": []},
        )
    )

    client = MeshClient(GATEWAY)
    results = client.discover("nonexistent/type")
    client.close()

    assert results == []


@respx.mock
def test_health():
    respx.get(f"{GATEWAY}/health").mock(
        return_value=httpx.Response(
            200,
            json={
                "status": "ok",
                "identity": "did:key:z6Mktest",
                "seed": "1.2.3.4:4433",
            },
        )
    )

    client = MeshClient(GATEWAY)
    health = client.health()
    client.close()

    assert isinstance(health, HealthStatus)
    assert health.status == "ok"
    assert health.identity == "did:key:z6Mktest"
    assert health.seed == "1.2.3.4:4433"


@respx.mock
def test_publish_error():
    respx.post(f"{GATEWAY}/v1/publish").mock(
        return_value=httpx.Response(
            400,
            json={"error": "payload encoding failed: missing field"},
        )
    )

    client = MeshClient(GATEWAY)
    with pytest.raises(MeshError) as exc_info:
        client.publish("bad-type", endpoint="")
    client.close()

    assert exc_info.value.status_code == 400
    assert "payload encoding failed" in exc_info.value.message


@respx.mock
def test_context_manager():
    respx.get(f"{GATEWAY}/health").mock(
        return_value=httpx.Response(
            200,
            json={
                "status": "ok",
                "identity": "did:key:z6Mktest",
                "seed": "1.2.3.4:4433",
            },
        )
    )

    with MeshClient(GATEWAY) as client:
        health = client.health()
        assert health.status == "ok"


# ── Async tests ─────────────────────────────────────────────────────────


@respx.mock
@pytest.mark.asyncio
async def test_async_publish():
    respx.post(f"{GATEWAY}/v1/publish").mock(
        return_value=httpx.Response(
            200,
            json={"ok": True, "descriptor_id": "blake3:def456"},
        )
    )

    async with AsyncMeshClient(GATEWAY) as client:
        result = await client.publish(
            "storage/blob",
            endpoint="https://storage.example.com/v1/upload",
        )

    assert isinstance(result, PublishResult)
    assert result.ok is True
    assert result.descriptor_id == "blake3:def456"


@respx.mock
@pytest.mark.asyncio
async def test_async_discover():
    respx.get(f"{GATEWAY}/v1/discover").mock(
        return_value=httpx.Response(
            200,
            json={"descriptors": [SAMPLE_DESCRIPTOR]},
        )
    )

    async with AsyncMeshClient(GATEWAY) as client:
        results = await client.discover("compute/inference")

    assert len(results) == 1
    assert results[0].type == "compute/inference/text-generation"
