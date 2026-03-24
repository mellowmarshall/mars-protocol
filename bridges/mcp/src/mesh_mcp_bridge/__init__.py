"""Bidirectional bridge between MCP servers and the mesh network."""

from .publisher import MeshPublisher
from .server import MeshMcpServer

__version__ = "0.1.0"

__all__ = ["MeshPublisher", "MeshMcpServer", "__version__"]
