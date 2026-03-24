"""Publishes MCP server capabilities to the mesh network."""

from __future__ import annotations

import logging
from typing import Any

import httpx
from mcp import ClientSession, StdioServerParameters
from mcp.client.stdio import stdio_client

logger = logging.getLogger(__name__)


class MeshPublisher:
    """Publishes MCP server capabilities to the mesh network.

    Connects to an MCP server, enumerates its tools, and registers each one
    as a mesh descriptor so that other mesh participants can discover them.
    """

    def __init__(self, gateway_url: str):
        self.gateway_url = gateway_url
        self.client = httpx.Client(base_url=gateway_url, timeout=30)

    # ── Public API ──────────────────────────────────────────────────────

    async def publish_mcp_server(self, server_command: str, server_name: str) -> list[str]:
        """Connect to an MCP server, list its tools, and publish each as a mesh descriptor.

        Parameters
        ----------
        server_command:
            Command to launch the MCP server (e.g. ``python my_server.py``).
        server_name:
            Human-readable name used in the mesh descriptor metadata.

        Returns
        -------
        list[str]
            Descriptor IDs for every published tool.
        """
        parts = server_command.split()
        server_params = StdioServerParameters(command=parts[0], args=parts[1:])

        descriptor_ids: list[str] = []

        async with stdio_client(server_params) as (read, write):
            async with ClientSession(read, write) as session:
                await session.initialize()

                tools_result = await session.list_tools()
                logger.info(
                    "Discovered %d tools from MCP server %s",
                    len(tools_result.tools),
                    server_name,
                )

                for tool in tools_result.tools:
                    input_schema: dict[str, Any] = {}
                    if tool.inputSchema is not None:
                        # inputSchema is already a dict-like JSON Schema
                        input_schema = dict(tool.inputSchema)

                    descriptor_id = self.publish_tool(
                        server_url=server_command,
                        server_name=server_name,
                        tool_name=tool.name,
                        description=tool.description or "",
                        input_schema=input_schema,
                    )
                    descriptor_ids.append(descriptor_id)
                    logger.info("Published tool %s as %s", tool.name, descriptor_id)

        return descriptor_ids

    def publish_tool(
        self,
        server_url: str,
        server_name: str,
        tool_name: str,
        description: str,
        input_schema: dict[str, Any],
    ) -> str:
        """Publish a single MCP tool to the mesh.

        Returns the descriptor ID assigned by the gateway.
        """
        response = self.client.post(
            "/v1/publish",
            json={
                "type": f"mcp/tool/{tool_name}",
                "endpoint": server_url,
                "params": {
                    "server_name": server_name,
                    "tool_name": tool_name,
                    "description": description,
                    "input_schema": input_schema,
                    "protocol": "mcp",
                },
            },
        )
        response.raise_for_status()
        return response.json()["descriptor_id"]

    # ── Lifecycle ───────────────────────────────────────────────────────

    def close(self) -> None:
        self.client.close()

    def __enter__(self) -> "MeshPublisher":
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()
