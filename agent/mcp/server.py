"""FastMCP server configuration and lifecycle helpers."""

from __future__ import annotations

from typing import Any, Mapping

from fastmcp.exceptions import NotFoundError
from fastmcp.tools.tool import ToolResult

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
        raise RuntimeError("MCP tools schema has not been initialised")
    return _TOOLS_SCHEMA


async def call_tool(name: str, arguments: Mapping[str, Any]) -> Any:
    """Execute a tool by name with arguments."""

    logger.debug("mcp_tool_call", name=name, arguments=arguments)
    try:
        tool_result = await mcp._tool_manager.call_tool(name, dict(arguments))
    except NotFoundError as exc:  # pragma: no cover - defensive guard
        raise KeyError(f"Unknown tool: {name}") from exc

    return _serialize_tool_result(tool_result)


async def refresh_tools_schema() -> list[dict[str, Any]]:
    """Regenerate and cache tool schema."""

    global _TOOLS_SCHEMA
    tools = await mcp.get_tools()
    schema: list[dict[str, Any]] = []

    for tool in tools.values():
        if not tool.enabled:
            continue

        mcp_tool = tool.to_mcp_tool()
        parameters = mcp_tool.inputSchema or {"type": "object", "properties": {}}
        schema.append(
            {
                "type": "function",
                "function": {
                    "name": mcp_tool.name,
                    "description": mcp_tool.description or "",
                    "parameters": parameters,
                },
            }
        )

    _TOOLS_SCHEMA = schema
    logger.info("mcp_tools_schema_loaded", count=len(_TOOLS_SCHEMA))
    return _TOOLS_SCHEMA


def _serialize_tool_result(tool_result: ToolResult) -> Any:
    """Convert FastMCP ToolResult into JSON-serialisable payload."""

    if tool_result.structured_content is not None:
        payload = tool_result.structured_content
        if (
            isinstance(payload, dict)
            and set(payload.keys()) == {"result"}
        ):
            return payload["result"]
        return payload

    serialised_blocks: list[Any] = []
    for block in tool_result.content:
        if hasattr(block, "model_dump"):
            serialised_blocks.append(block.model_dump())
        else:  # pragma: no cover
            serialised_blocks.append(str(block))
    if len(serialised_blocks) == 1:
        return serialised_blocks[0]
    return serialised_blocks
