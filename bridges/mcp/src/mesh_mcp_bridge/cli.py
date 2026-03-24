"""CLI entrypoint for the mesh-mcp-bridge."""

from __future__ import annotations

import asyncio
import logging
import sys

import click

from .publisher import MeshPublisher
from .server import MeshMcpServer


@click.group()
@click.option("--verbose", "-v", is_flag=True, help="Enable debug logging.")
def main(verbose: bool) -> None:
    """Bidirectional bridge between MCP servers and the mesh network."""
    level = logging.DEBUG if verbose else logging.INFO
    logging.basicConfig(
        level=level,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        stream=sys.stderr,
    )


@main.command()
@click.option(
    "--gateway",
    required=True,
    help="Mesh gateway URL (e.g. http://localhost:3000).",
)
@click.option(
    "--mcp-server",
    required=True,
    help="Command to launch the MCP server (e.g. 'python my_server.py').",
)
@click.option(
    "--name",
    default=None,
    help="Human-readable name for the MCP server. Defaults to the command.",
)
def publish(gateway: str, mcp_server: str, name: str | None) -> None:
    """Connect to an MCP server and publish its tools to the mesh."""
    server_name = name or mcp_server

    with MeshPublisher(gateway) as pub:
        ids = asyncio.run(pub.publish_mcp_server(mcp_server, server_name))

    click.echo(f"Published {len(ids)} tool(s) to the mesh:")
    for descriptor_id in ids:
        click.echo(f"  {descriptor_id}")


@main.command()
@click.option(
    "--gateway",
    required=True,
    help="Mesh gateway URL (e.g. http://localhost:3000).",
)
@click.option(
    "--port",
    default=8080,
    show_default=True,
    help="Port for the HTTP transport. Ignored when --transport=stdio.",
)
@click.option(
    "--host",
    default="127.0.0.1",
    show_default=True,
    help="Bind address for the HTTP transport.",
)
@click.option(
    "--filter",
    "capability_filter",
    default="mcp/tool",
    show_default=True,
    help="Descriptor type prefix to discover from the mesh.",
)
@click.option(
    "--transport",
    type=click.Choice(["stdio", "http"]),
    default="stdio",
    show_default=True,
    help="MCP transport to use.",
)
def serve(
    gateway: str,
    port: int,
    host: str,
    capability_filter: str,
    transport: str,
) -> None:
    """Start an MCP server that discovers tools from the mesh."""
    mesh_server = MeshMcpServer(gateway, capability_filter=capability_filter)

    if transport == "http":
        click.echo(f"Starting MCP HTTP server on {host}:{port}", err=True)
        asyncio.run(mesh_server.run_http(host=host, port=port))
    else:
        click.echo("Starting MCP stdio server", err=True)
        asyncio.run(mesh_server.run_stdio())


if __name__ == "__main__":
    main()
