"""Shared helpers for MCP tool implementations."""

from __future__ import annotations

from typing import Any

from ...core.exceptions import ExternalServiceError, is_retryable_external_error


def wrap_response(response: Any) -> dict[str, Any]:
    """Ensure OKX responses are returned as dictionaries."""

    if isinstance(response, dict):
        return response
    return {"data": response}


def wrap_external_error(tool_name: str, exc: ExternalServiceError) -> dict[str, Any]:
    """Return a structured tool error without making FastMCP print a traceback."""

    return {
        "ok": False,
        "error": {
            "code": "external_service_error",
            "tool": tool_name,
            "type": type(exc).__name__,
            "message": str(exc),
            "retryable": is_retryable_external_error(exc),
        },
    }
