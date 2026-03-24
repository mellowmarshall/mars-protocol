"""Core bridge logic shared between publisher and server components."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any


@dataclass(frozen=True)
class MeshDescriptor:
    """A capability descriptor as returned by the mesh gateway."""

    id: str
    publisher: str
    type: str
    endpoint: str
    params: dict[str, Any]
    timestamp: int = 0
    ttl: int = 0
    sequence: int = 0

    @property
    def tool_name(self) -> str:
        """Extract the MCP tool name from the descriptor type.

        Descriptor types follow the pattern ``mcp/tool/{tool_name}``.
        """
        prefix = "mcp/tool/"
        if self.type.startswith(prefix):
            return self.type[len(prefix):]
        return self.type

    @property
    def description(self) -> str:
        return self.params.get("description", "")

    @property
    def input_schema(self) -> dict[str, Any]:
        return self.params.get("input_schema", {"type": "object", "properties": {}})


def descriptor_from_json(data: dict[str, Any]) -> MeshDescriptor:
    """Create a ``MeshDescriptor`` from a gateway JSON response dict."""
    return MeshDescriptor(
        id=data.get("id", ""),
        publisher=data.get("publisher", ""),
        type=data.get("type", ""),
        endpoint=data.get("endpoint", ""),
        params=data.get("params", {}),
        timestamp=data.get("timestamp", 0),
        ttl=data.get("ttl", 0),
        sequence=data.get("sequence", 0),
    )
