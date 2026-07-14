"""N2-B ToolAdapterRegistry — inspection/planning only, never execution."""
from __future__ import annotations

from . import dotnet_adapter, gradle_adapter, maven_adapter, python_adapter, rust_adapter

ADAPTERS = {
    "dotnet": dotnet_adapter,
    "rust": rust_adapter,
    "python": python_adapter,
    "jvm-maven": maven_adapter,
    "jvm-gradle": gradle_adapter,
}


def get_adapter(ecosystem: str):
    if ecosystem not in ADAPTERS:
        raise ValueError(f"no adapter registered for ecosystem {ecosystem!r}; known: {sorted(ADAPTERS)}")
    return ADAPTERS[ecosystem]


def registered_ecosystems() -> list[str]:
    return sorted(ADAPTERS)
