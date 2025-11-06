"""Market data MCP tools."""

from typing import Any

from ...core.okx_client import okx_client
from ..registry import mcp


def _wrap_response(response: Any) -> dict[str, Any]:
    if isinstance(response, dict):
        return response
    return {"data": response}


@mcp.tool()
async def get_ticker(inst_id: str) -> dict[str, Any]:
    """获取指定交易对的最新行情快照。"""

    response = await okx_client.get_ticker(inst_id)
    return _wrap_response(response)


@mcp.tool()
async def get_order_book(inst_id: str, depth: int = 5) -> dict[str, Any]:
    """查询指定交易对的委托簿深度。"""

    response = await okx_client.get_order_book(inst_id, depth=depth)
    return _wrap_response(response)


@mcp.tool()
async def get_candles(inst_id: str, bar: str = "1m", limit: int = 100) -> dict[str, Any]:
    """拉取指定交易对的 K 线数据。"""

    response = await okx_client.get_candles(inst_id, bar=bar, limit=limit)
    return _wrap_response(response)
