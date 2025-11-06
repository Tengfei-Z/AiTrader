"""FastMCP server placeholder."""

from typing import Any, Mapping

from fastmcp import FastMCP

from ..core.logging_config import get_logger
from ..core.okx_client import okx_client

logger = get_logger(__name__)

mcp = FastMCP("aitrader-agent")
_TOOLS_SCHEMA: list[dict[str, Any]] | None = None


@mcp.tool()
async def get_ticker(inst_id: str) -> dict[str, Any]:
    """Fetch ticker information through OKX."""

    response = await okx_client.get_ticker(inst_id)
    if isinstance(response, dict):
        return response
    return {"data": response}


@mcp.tool()
async def get_order_book(inst_id: str, depth: int = 5) -> dict[str, Any]:
    """Fetch order book depth."""

    response = await okx_client.get_order_book(inst_id, depth=depth)
    return response if isinstance(response, dict) else {"data": response}


@mcp.tool()
async def get_candles(inst_id: str, bar: str = "1m", limit: int = 100) -> dict[str, Any]:
    """Fetch historical candles."""

    response = await okx_client.get_candles(inst_id, bar=bar, limit=limit)
    return response if isinstance(response, dict) else {"data": response}


@mcp.tool()
async def get_balance() -> dict[str, Any]:
    """Fetch account balance."""

    response = await okx_client.get_account_balance()
    return response if isinstance(response, dict) else {"data": response}


@mcp.tool()
async def get_positions(inst_type: str | None = None) -> dict[str, Any]:
    """Fetch current positions."""

    response = await okx_client.get_positions(inst_type=inst_type)
    return response if isinstance(response, dict) else {"data": response}


@mcp.tool()
async def get_open_orders(inst_type: str | None = None) -> dict[str, Any]:
    """Fetch pending orders."""

    response = await okx_client.get_open_orders(inst_type=inst_type)
    return response if isinstance(response, dict) else {"data": response}


@mcp.tool()
async def place_order(order: dict[str, Any]) -> dict[str, Any]:
    """Place an order."""

    response = await okx_client.place_order(order)
    return response if isinstance(response, dict) else {"data": response}


@mcp.tool()
async def cancel_order(inst_id: str, order_id: str | None = None, client_order_id: str | None = None) -> dict[str, Any]:
    """Cancel an order."""

    response = await okx_client.cancel_order(
        inst_id=inst_id, order_id=order_id, client_order_id=client_order_id
    )
    return response if isinstance(response, dict) else {"data": response}


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
