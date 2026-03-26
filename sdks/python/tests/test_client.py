"""Tests for the Mesh Protocol Python client."""

from __future__ import annotations

import httpx
import pytest
import respx

from mesh_protocol import (
    AsyncMaintainedDescriptor,
    AsyncMeshClient,
    Descriptor,
    HealthStatus,
    MaintainedDescriptor,
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
    "params": {"model": "glm-5"},
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
        params={"model": "glm-5"},
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
    assert parsed["params"] == {"model": "glm-5"}


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
    assert d.params == {"model": "glm-5"}
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


# ── MaintainedDescriptor tests ─────────────────────────────────────────


@respx.mock
def test_publish_maintained_returns_maintained_descriptor():
    respx.post(f"{GATEWAY}/v1/publish").mock(
        return_value=httpx.Response(
            200,
            json={"ok": True, "descriptor_id": "blake3:maintained1"},
        )
    )

    client = MeshClient(GATEWAY)
    maintained = client.publish_maintained(
        "compute/inference/text-generation",
        endpoint="https://agent.example.com/v1/generate",
        refresh_interval=3600.0,  # long interval so thread just waits
    )

    try:
        assert isinstance(maintained, MaintainedDescriptor)
        assert maintained.descriptor_id == "blake3:maintained1"
        assert maintained.capability_type == "compute/inference/text-generation"
        assert maintained.endpoint == "https://agent.example.com/v1/generate"
    finally:
        maintained.stop()
        client.close()


@respx.mock
def test_publish_maintained_stop():
    respx.post(f"{GATEWAY}/v1/publish").mock(
        return_value=httpx.Response(
            200,
            json={"ok": True, "descriptor_id": "blake3:stop1"},
        )
    )

    client = MeshClient(GATEWAY)
    maintained = client.publish_maintained(
        "compute/inference",
        endpoint="https://agent.example.com/v1/generate",
        refresh_interval=3600.0,
    )

    assert maintained.is_alive() is True
    maintained.stop()
    assert maintained.is_alive() is False
    client.close()


@respx.mock
def test_publish_maintained_is_alive():
    respx.post(f"{GATEWAY}/v1/publish").mock(
        return_value=httpx.Response(
            200,
            json={"ok": True, "descriptor_id": "blake3:alive1"},
        )
    )

    client = MeshClient(GATEWAY)
    maintained = client.publish_maintained(
        "compute/inference",
        endpoint="https://agent.example.com/v1/generate",
        refresh_interval=3600.0,
    )

    try:
        assert maintained.is_alive() is True
    finally:
        maintained.stop()
        assert maintained.is_alive() is False
        client.close()


@respx.mock
def test_publish_maintained_context_manager():
    respx.post(f"{GATEWAY}/v1/publish").mock(
        return_value=httpx.Response(
            200,
            json={"ok": True, "descriptor_id": "blake3:ctx1"},
        )
    )

    client = MeshClient(GATEWAY)
    with client.publish_maintained(
        "compute/inference",
        endpoint="https://agent.example.com/v1/generate",
        refresh_interval=3600.0,
    ) as maintained:
        assert isinstance(maintained, MaintainedDescriptor)
        assert maintained.is_alive() is True
        assert maintained.descriptor_id == "blake3:ctx1"

    # After exiting the context manager, thread should be stopped.
    assert maintained.is_alive() is False
    client.close()


# ── AsyncMaintainedDescriptor tests ────────────────────────────────────


@respx.mock
@pytest.mark.asyncio
async def test_async_publish_maintained_returns_descriptor():
    respx.post(f"{GATEWAY}/v1/publish").mock(
        return_value=httpx.Response(
            200,
            json={"ok": True, "descriptor_id": "blake3:async_m1"},
        )
    )

    async with AsyncMeshClient(GATEWAY) as client:
        maintained = await client.publish_maintained(
            "compute/inference/text-generation",
            endpoint="https://agent.example.com/v1/generate",
            refresh_interval=3600.0,
        )
        try:
            assert isinstance(maintained, AsyncMaintainedDescriptor)
            assert maintained.descriptor_id == "blake3:async_m1"
            assert maintained.capability_type == "compute/inference/text-generation"
            assert maintained.endpoint == "https://agent.example.com/v1/generate"
            assert maintained.is_alive() is True
        finally:
            await maintained.stop()


@respx.mock
@pytest.mark.asyncio
async def test_async_publish_maintained_stop():
    respx.post(f"{GATEWAY}/v1/publish").mock(
        return_value=httpx.Response(
            200,
            json={"ok": True, "descriptor_id": "blake3:async_stop1"},
        )
    )

    async with AsyncMeshClient(GATEWAY) as client:
        maintained = await client.publish_maintained(
            "compute/inference",
            endpoint="https://agent.example.com/v1/generate",
            refresh_interval=3600.0,
        )
        assert maintained.is_alive() is True
        await maintained.stop()
        assert maintained.is_alive() is False


@respx.mock
@pytest.mark.asyncio
async def test_async_publish_maintained_context_manager():
    respx.post(f"{GATEWAY}/v1/publish").mock(
        return_value=httpx.Response(
            200,
            json={"ok": True, "descriptor_id": "blake3:async_ctx1"},
        )
    )

    async with AsyncMeshClient(GATEWAY) as client:
        async with await client.publish_maintained(
            "compute/inference",
            endpoint="https://agent.example.com/v1/generate",
            refresh_interval=3600.0,
        ) as maintained:
            assert isinstance(maintained, AsyncMaintainedDescriptor)
            assert maintained.is_alive() is True
            assert maintained.descriptor_id == "blake3:async_ctx1"

        assert maintained.is_alive() is False
