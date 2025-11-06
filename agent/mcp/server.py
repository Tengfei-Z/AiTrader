"""FastMCP server configuration and lifecycle helpers."""

from typing import Any, Mapping

from ..core.logging_config import get_logger
from .registry import mcp

# Import tool modules so decorators run at import time.
from . import tools  # noqa: F401

logger = get_logger(__name__)

_TOOLS_SCHEMA: list[dict[str, Any]] | None = None


def get_tools_schema() -> list[dict[str, Any]]:
    """Expose cached MCP tool schema for the LLM client."""

    global _TOOLS_SCHEMA
    if _TOOLS_SCHEMA is None:
        _TOOLS_SCHEMA = refresh_tools_schema()
    return _TOOLS_SCHEMA


async def call_tool(name: str, arguments: Mapping[str, Any]) -> Any:
    """Execute a tool by name with arguments."""

    logger.debug("mcp_tool_call", name=name, arguments=arguments)
    return await mcp.call_tool(name, dict(arguments))


def refresh_tools_schema() -> list[dict[str, Any]]:
    """Regenerate and cache tool schema."""

    global _TOOLS_SCHEMA
    _TOOLS_SCHEMA = mcp.get_tools_schema()
    logger.info("mcp_tools_schema_loaded", count=len(_TOOLS_SCHEMA))
    return _TOOLS_SCHEMA
