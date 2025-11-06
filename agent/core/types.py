"""Shared type definitions."""

from dataclasses import dataclass
from datetime import datetime
from typing import Any, Mapping


@dataclass(slots=True)
class ToolInvocationResult:
    """Represents the outcome returned to the LLM after executing a tool."""

    name: str
    arguments: Mapping[str, Any]
    result: Any
    latency_ms: float
    timestamp: datetime
