"""Mesh Protocol client — publish and discover capabilities on the mesh network."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Optional

import httpx


# ── Data types ──────────────────────────────────────────────────────────


@dataclass
class Descriptor:
    """A capability descriptor returned by the mesh network."""

    id: str
    publisher: str
    type: str
    endpoint: str
    params: Optional[dict[str, Any]]
    timestamp: int
    ttl: int
    sequence: int


@dataclass
class PublishResult:
    """Acknowledgement returned after a successful publish."""

    ok: bool
    descriptor_id: str


@dataclass
class HealthStatus:
    """Health-check response from the mesh gateway."""

    status: str
    identity: str
    seed: str


# ── Exceptions ──────────────────────────────────────────────────────────


class MeshError(Exception):
    """Raised when the mesh gateway returns an error response."""

    def __init__(self, status_code: int, message: str) -> None:
        self.status_code = status_code
        self.message = message
        super().__init__(f"HTTP {status_code}: {message}")


# ── Helpers ─────────────────────────────────────────────────────────────


def _raise_for_error(response: httpx.Response) -> None:
    """Raise a MeshError if the response indicates failure."""
    if response.status_code >= 400:
        try:
            body = response.json()
            message = body.get("error", response.text)
        except Exception:
            message = response.text
        raise MeshError(response.status_code, message)


def _parse_descriptor(data: dict[str, Any]) -> Descriptor:
    return Descriptor(
        id=data["id"],
        publisher=data["publisher"],
        type=data["type"],
        endpoint=data["endpoint"],
        params=data.get("params"),
        timestamp=data["timestamp"],
        ttl=data["ttl"],
        sequence=data["sequence"],
    )


# ── Synchronous client ──────────────────────────────────────────────────


class MeshClient:
    """Synchronous client for the Capability Mesh Protocol.

    Connects to a mesh-gateway HTTP endpoint to publish and discover
    capabilities on the decentralized mesh network.

    Usage::

        client = MeshClient("http://localhost:3000")
        client.publish(
            "compute/inference/text-generation",
            endpoint="https://my-agent.example.com/v1/generate",
            params={"model": "llama-4-scout"},
        )
        providers = client.discover("compute/inference")
    """

    def __init__(self, gateway_url: str, *, timeout: float = 30.0) -> None:
        self._base_url = gateway_url.rstrip("/")
        self._client = httpx.Client(base_url=self._base_url, timeout=timeout)

    # -- public API -------------------------------------------------------

    def publish(
        self,
        capability_type: str,
        endpoint: str,
        params: Optional[dict[str, Any]] = None,
    ) -> PublishResult:
        """Publish a capability descriptor to the mesh network.

        Args:
            capability_type: Hierarchical type string (e.g. ``"compute/inference/text-generation"``).
            endpoint: URL where the capability is served.
            params: Optional metadata attached to the descriptor.

        Returns:
            A :class:`PublishResult` with the assigned descriptor id.

        Raises:
            MeshError: If the gateway rejects the request.
        """
        body: dict[str, Any] = {"type": capability_type, "endpoint": endpoint}
        if params is not None:
            body["params"] = params
        response = self._client.post("/v1/publish", json=body)
        _raise_for_error(response)
        data = response.json()
        return PublishResult(ok=data["ok"], descriptor_id=data["descriptor_id"])

    def discover(self, capability_type: str) -> list[Descriptor]:
        """Discover capability descriptors matching a type prefix.

        Args:
            capability_type: Type prefix to search for (e.g. ``"compute/inference"``).

        Returns:
            A list of matching :class:`Descriptor` objects (may be empty).

        Raises:
            MeshError: If the gateway returns an error.
        """
        response = self._client.get("/v1/discover", params={"type": capability_type})
        _raise_for_error(response)
        data = response.json()
        return [_parse_descriptor(d) for d in data["descriptors"]]

    def health(self) -> HealthStatus:
        """Check gateway health.

        Returns:
            A :class:`HealthStatus` with the gateway's identity and seed info.

        Raises:
            MeshError: If the gateway is unreachable or unhealthy.
        """
        response = self._client.get("/health")
        _raise_for_error(response)
        data = response.json()
        return HealthStatus(
            status=data["status"],
            identity=data["identity"],
            seed=data["seed"],
        )

    def close(self) -> None:
        """Close the underlying HTTP connection."""
        self._client.close()

    # -- context manager ---------------------------------------------------

    def __enter__(self) -> MeshClient:
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()


# ── Asynchronous client ─────────────────────────────────────────────────


class AsyncMeshClient:
    """Async client for the Capability Mesh Protocol.

    Same API as :class:`MeshClient` but all methods are coroutines.

    Usage::

        async with AsyncMeshClient("http://localhost:3000") as client:
            await client.publish(
                "compute/inference/text-generation",
                endpoint="https://my-agent.example.com/v1/generate",
            )
            providers = await client.discover("compute/inference")
    """

    def __init__(self, gateway_url: str, *, timeout: float = 30.0) -> None:
        self._base_url = gateway_url.rstrip("/")
        self._client = httpx.AsyncClient(base_url=self._base_url, timeout=timeout)

    # -- public API -------------------------------------------------------

    async def publish(
        self,
        capability_type: str,
        endpoint: str,
        params: Optional[dict[str, Any]] = None,
    ) -> PublishResult:
        """Publish a capability descriptor to the mesh network."""
        body: dict[str, Any] = {"type": capability_type, "endpoint": endpoint}
        if params is not None:
            body["params"] = params
        response = await self._client.post("/v1/publish", json=body)
        _raise_for_error(response)
        data = response.json()
        return PublishResult(ok=data["ok"], descriptor_id=data["descriptor_id"])

    async def discover(self, capability_type: str) -> list[Descriptor]:
        """Discover capability descriptors matching a type prefix."""
        response = await self._client.get(
            "/v1/discover", params={"type": capability_type}
        )
        _raise_for_error(response)
        data = response.json()
        return [_parse_descriptor(d) for d in data["descriptors"]]

    async def health(self) -> HealthStatus:
        """Check gateway health."""
        response = await self._client.get("/health")
        _raise_for_error(response)
        data = response.json()
        return HealthStatus(
            status=data["status"],
            identity=data["identity"],
            seed=data["seed"],
        )

    async def close(self) -> None:
        """Close the underlying HTTP connection."""
        await self._client.aclose()

    # -- context manager ---------------------------------------------------

    async def __aenter__(self) -> AsyncMeshClient:
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.close()
