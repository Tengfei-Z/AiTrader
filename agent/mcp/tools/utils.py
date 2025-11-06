"""Shared helpers for MCP tool implementations."""

from __future__ import annotations

from typing import Any


def wrap_response(response: Any) -> dict[str, Any]:
    """Ensure OKX responses are returned as dictionaries."""

    if isinstance(response, dict):
        return response
    return {"data": response}
