"""Mesh Protocol Python SDK — publish and discover capabilities on the mesh network."""

from .client import (
    AsyncMaintainedDescriptor,
    AsyncMeshClient,
    Descriptor,
    HealthStatus,
    MaintainedDescriptor,
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
    "MaintainedDescriptor",
    "AsyncMaintainedDescriptor",
]
