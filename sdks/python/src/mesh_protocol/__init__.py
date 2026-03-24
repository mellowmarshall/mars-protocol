"""Mesh Protocol Python SDK — publish and discover capabilities on the mesh network."""

from .client import (
    AsyncMeshClient,
    Descriptor,
    HealthStatus,
    MeshClient,
    MeshError,
    PublishResult,
)

__version__ = "0.1.0"
__all__ = [
    "MeshClient",
    "AsyncMeshClient",
    "Descriptor",
    "PublishResult",
    "HealthStatus",
    "MeshError",
]
