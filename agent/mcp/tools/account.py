"""Account-related MCP tools."""

from typing import Any

from ...core.okx_client import okx_client
from ..registry import mcp
from .market import _wrap_response


@mcp.tool()
async def get_balance() -> dict[str, Any]:
    """查询账户余额信息。"""

    response = await okx_client.get_account_balance()
    return _wrap_response(response)


@mcp.tool()
async def get_positions(inst_type: str | None = None) -> dict[str, Any]:
    """查询持仓信息，可按产品类型过滤。"""

    response = await okx_client.get_positions(inst_type=inst_type)
    return _wrap_response(response)


@mcp.tool()
async def get_open_orders(inst_type: str | None = None) -> dict[str, Any]:
    """查询未成交委托列表。"""

    response = await okx_client.get_open_orders(inst_type=inst_type)
    return _wrap_response(response)
