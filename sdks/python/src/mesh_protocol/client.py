"""Mesh Protocol client — publish and discover capabilities on the mesh network."""

from __future__ import annotations

import asyncio
import logging
import threading
from dataclasses import dataclass, field
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


# ── Maintained descriptors ─────────────────────────────────────────────

_log = logging.getLogger("mesh_protocol.maintained")


@dataclass
class MaintainedDescriptor:
    """A published descriptor that is automatically refreshed in the background."""

    descriptor_id: str
    capability_type: str
    endpoint: str

    _thread: threading.Thread = field(repr=False, default=None)  # type: ignore[assignment]
    _stop_event: threading.Event = field(repr=False, default=None)  # type: ignore[assignment]
    _last_error: Optional[str] = field(repr=False, default=None)

    def stop(self) -> None:
        """Signal the refresh thread to stop and wait for it to exit."""
        if self._stop_event is not None:
            self._stop_event.set()
        if self._thread is not None:
            self._thread.join(timeout=5.0)

    def is_alive(self) -> bool:
        """Return True if the background refresh thread is still running."""
        return self._thread is not None and self._thread.is_alive()

    def __enter__(self) -> MaintainedDescriptor:
        return self

    def __exit__(self, *exc: object) -> None:
        self.stop()


@dataclass
class AsyncMaintainedDescriptor:
    """A published descriptor that is automatically refreshed via an async task."""

    descriptor_id: str
    capability_type: str
    endpoint: str

    _task: asyncio.Task[None] = field(repr=False, default=None)  # type: ignore[assignment]
    _last_error: Optional[str] = field(repr=False, default=None)

    async def stop(self) -> None:
        """Cancel the refresh task and wait for it to finish."""
        if self._task is not None:
            self._task.cancel()
            try:
                await self._task
            except asyncio.CancelledError:
                pass

    def is_alive(self) -> bool:
        """Return True if the background refresh task is still running."""
        return self._task is not None and not self._task.done()

    async def __aenter__(self) -> AsyncMaintainedDescriptor:
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.stop()


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
            params={"model": "glm-5"},
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

    def publish_maintained(
        self,
        capability_type: str,
        endpoint: str,
        params: Optional[dict[str, Any]] = None,
        refresh_interval: float = 1800.0,
        max_retries: int = 10,
    ) -> MaintainedDescriptor:
        """Publish a descriptor and keep it alive via a background daemon thread.

        The descriptor is published immediately.  A background thread then
        re-publishes at ``refresh_interval`` seconds to prevent expiry.

        Args:
            capability_type: Hierarchical type string.
            endpoint: URL where the capability is served.
            params: Optional metadata attached to the descriptor.
            refresh_interval: Seconds between refresh publishes (default 30 min).
            max_retries: After this many consecutive failures the thread logs an
                error but keeps retrying — it never gives up.

        Returns:
            A :class:`MaintainedDescriptor` that can be stopped via ``stop()``
            or used as a context manager.
        """
        result = self.publish(capability_type, endpoint, params)

        stop_event = threading.Event()
        maintained = MaintainedDescriptor(
            descriptor_id=result.descriptor_id,
            capability_type=capability_type,
            endpoint=endpoint,
            _stop_event=stop_event,
        )

        def _refresh_loop() -> None:
            consecutive_failures = 0
            while True:
                stop_event.wait(timeout=refresh_interval)
                if stop_event.is_set():
                    _log.debug("Stop event set — exiting refresh loop for %s", maintained.descriptor_id)
                    return
                try:
                    new_result = self.publish(capability_type, endpoint, params)
                    maintained.descriptor_id = new_result.descriptor_id
                    maintained._last_error = None
                    consecutive_failures = 0
                    _log.debug("Refreshed descriptor %s", maintained.descriptor_id)
                except Exception as exc:
                    consecutive_failures += 1
                    maintained._last_error = str(exc)
                    if consecutive_failures >= max_retries:
                        _log.error(
                            "Refresh failed %d consecutive times for %s: %s",
                            consecutive_failures,
                            maintained.descriptor_id,
                            exc,
                        )
                    else:
                        _log.warning(
                            "Refresh attempt %d failed for %s: %s",
                            consecutive_failures,
                            maintained.descriptor_id,
                            exc,
                        )
                    # Exponential backoff: 5, 10, 20, 40, ... capped at 300s
                    backoff = min(5.0 * (2 ** (consecutive_failures - 1)), 300.0)
                    stop_event.wait(timeout=backoff)
                    if stop_event.is_set():
                        return

        thread = threading.Thread(target=_refresh_loop, daemon=True)
        thread.start()
        maintained._thread = thread
        return maintained

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

    async def publish_maintained(
        self,
        capability_type: str,
        endpoint: str,
        params: Optional[dict[str, Any]] = None,
        refresh_interval: float = 1800.0,
        max_retries: int = 10,
    ) -> AsyncMaintainedDescriptor:
        """Publish a descriptor and keep it alive via a background async task.

        The descriptor is published immediately.  A background task then
        re-publishes at ``refresh_interval`` seconds to prevent expiry.

        Args:
            capability_type: Hierarchical type string.
            endpoint: URL where the capability is served.
            params: Optional metadata attached to the descriptor.
            refresh_interval: Seconds between refresh publishes (default 30 min).
            max_retries: After this many consecutive failures the task logs an
                error but keeps retrying — it never gives up.

        Returns:
            An :class:`AsyncMaintainedDescriptor` that can be stopped via
            ``await stop()`` or used as an async context manager.
        """
        result = await self.publish(capability_type, endpoint, params)

        maintained = AsyncMaintainedDescriptor(
            descriptor_id=result.descriptor_id,
            capability_type=capability_type,
            endpoint=endpoint,
        )

        async def _refresh_loop() -> None:
            consecutive_failures = 0
            while True:
                await asyncio.sleep(refresh_interval)
                try:
                    new_result = await self.publish(capability_type, endpoint, params)
                    maintained.descriptor_id = new_result.descriptor_id
                    maintained._last_error = None
                    consecutive_failures = 0
                    _log.debug("Refreshed descriptor %s", maintained.descriptor_id)
                except asyncio.CancelledError:
                    _log.debug("Refresh task cancelled for %s", maintained.descriptor_id)
                    raise
                except Exception as exc:
                    consecutive_failures += 1
                    maintained._last_error = str(exc)
                    if consecutive_failures >= max_retries:
                        _log.error(
                            "Refresh failed %d consecutive times for %s: %s",
                            consecutive_failures,
                            maintained.descriptor_id,
                            exc,
                        )
                    else:
                        _log.warning(
                            "Refresh attempt %d failed for %s: %s",
                            consecutive_failures,
                            maintained.descriptor_id,
                            exc,
                        )
                    # Exponential backoff: 5, 10, 20, 40, ... capped at 300s
                    backoff = min(5.0 * (2 ** (consecutive_failures - 1)), 300.0)
                    try:
                        await asyncio.sleep(backoff)
                    except asyncio.CancelledError:
                        raise

        maintained._task = asyncio.create_task(_refresh_loop())
        return maintained

    async def close(self) -> None:
        """Close the underlying HTTP connection."""
        await self._client.aclose()

    # -- context manager ---------------------------------------------------

    async def __aenter__(self) -> AsyncMeshClient:
        return self

    async def __aexit__(self, *exc: object) -> None:
        await self.close()
