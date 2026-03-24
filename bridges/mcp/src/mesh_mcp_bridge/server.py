"""MCP server that discovers capabilities from the mesh network.

MCP-compatible agents connect to this server and see mesh capabilities
as standard MCP tools.  When a tool is invoked the server forwards the
request to the actual endpoint advertised in the mesh descriptor.
"""

from __future__ import annotations

import json
import logging
import time
from collections.abc import AsyncIterator
from contextlib import asynccontextmanager
from typing import Any

import httpx
from mcp.server import Server
from mcp.types import TextContent, Tool

from .bridge import MeshDescriptor, descriptor_from_json

logger = logging.getLogger(__name__)

# How long (seconds) the discovered tool list is cached before re-fetching.
_CACHE_TTL = 60


class MeshMcpServer:
    """MCP server that discovers capabilities from the mesh network.

    MCP-compatible agents connect to this server and see mesh capabilities
    as standard MCP tools.  When a tool is invoked, the server forwards
    the request to the actual endpoint advertised in the mesh descriptor.
    """

    def __init__(self, gateway_url: str, capability_filter: str = "mcp/tool"):
        self.gateway_url = gateway_url
        self.capability_filter = capability_filter
        self.client = httpx.AsyncClient(base_url=gateway_url, timeout=30)

        # Cached descriptors
        self._descriptors: list[MeshDescriptor] = []
        self._cache_ts: float = 0.0

        # Build the MCP Server instance
        self.server = Server("mesh-discovery")
        self._register_handlers()

    # ── MCP handler registration ────────────────────────────────────────

    def _register_handlers(self) -> None:
        @self.server.list_tools()
        async def list_tools() -> list[Tool]:
            descriptors = await self._discover()
            return [self._descriptor_to_tool(d) for d in descriptors]

        @self.server.call_tool()
        async def call_tool(name: str, arguments: dict[str, Any]) -> list[TextContent]:
            descriptors = await self._discover()
            descriptor = self._find_descriptor(name, descriptors)
            if descriptor is None:
                return [TextContent(type="text", text=f"Unknown tool: {name}")]

            result = await self._forward_call(descriptor, arguments)
            return [TextContent(type="text", text=result)]

    # ── Mesh discovery ──────────────────────────────────────────────────

    async def _discover(self) -> list[MeshDescriptor]:
        """Query the mesh gateway for matching descriptors (cached)."""
        now = time.monotonic()
        if self._descriptors and (now - self._cache_ts) < _CACHE_TTL:
            return self._descriptors

        try:
            response = await self.client.get(
                "/v1/discover",
                params={"type": self.capability_filter},
            )
            response.raise_for_status()
            data = response.json()
            self._descriptors = [
                descriptor_from_json(d) for d in data.get("descriptors", [])
            ]
            self._cache_ts = now
            logger.info("Discovered %d descriptors from mesh", len(self._descriptors))
        except httpx.HTTPError as exc:
            logger.error("Mesh discovery failed: %s", exc)
            # Return stale cache on error rather than empty
        return self._descriptors

    # ── Conversion helpers ──────────────────────────────────────────────

    @staticmethod
    def _descriptor_to_tool(descriptor: MeshDescriptor) -> Tool:
        return Tool(
            name=descriptor.tool_name,
            description=descriptor.description,
            inputSchema=descriptor.input_schema,
        )

    @staticmethod
    def _find_descriptor(
        tool_name: str, descriptors: list[MeshDescriptor]
    ) -> MeshDescriptor | None:
        for d in descriptors:
            if d.tool_name == tool_name:
                return d
        return None

    # ── Forwarding ──────────────────────────────────────────────────────

    async def _forward_call(
        self, descriptor: MeshDescriptor, arguments: dict[str, Any]
    ) -> str:
        """Forward a tool invocation to the endpoint in the mesh descriptor.

        The endpoint is expected to accept a JSON POST with ``tool_name``
        and ``arguments`` and return a JSON body with a ``result`` field.
        """
        endpoint = descriptor.endpoint
        payload = {
            "tool_name": descriptor.tool_name,
            "arguments": arguments,
        }
        try:
            async with httpx.AsyncClient(timeout=60) as client:
                response = await client.post(endpoint, json=payload)
                response.raise_for_status()
                body = response.json()
                return json.dumps(body.get("result", body), indent=2)
        except httpx.HTTPError as exc:
            msg = f"Error forwarding call to {endpoint}: {exc}"
            logger.error(msg)
            return msg

    # ── Transport helpers ───────────────────────────────────────────────

    async def run_stdio(self) -> None:
        """Run the MCP server over stdio transport."""
        import mcp.server.stdio

        async with mcp.server.stdio.stdio_server() as (read_stream, write_stream):
            await self.server.run(
                read_stream,
                write_stream,
                self.server.create_initialization_options(),
            )

    async def run_http(self, host: str = "127.0.0.1", port: int = 8080) -> None:
        """Run the MCP server over Streamable HTTP transport."""
        import uvicorn
        from starlette.applications import Starlette
        from starlette.routing import Mount

        from mcp.server.streamable_http_manager import StreamableHTTPSessionManager

        session_manager = StreamableHTTPSessionManager(app=self.server)

        @asynccontextmanager
        async def lifespan(_app: Starlette) -> AsyncIterator[None]:
            async with session_manager.run():
                yield

        starlette_app = Starlette(
            routes=[Mount("/mcp", app=session_manager.handle_request)],
            lifespan=lifespan,
        )

        config = uvicorn.Config(starlette_app, host=host, port=port, log_level="info")
        server = uvicorn.Server(config)
        await server.serve()

    # ── Lifecycle ───────────────────────────────────────────────────────

    async def close(self) -> None:
        await self.client.aclose()
