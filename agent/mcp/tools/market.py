"""Market data MCP tools."""

from typing import Any

from ...core.okx_client import okx_client
from ..registry import mcp
from .utils import wrap_response


@mcp.tool()
async def get_ticker(inst_id: str) -> dict[str, Any]:
    """获取指定交易对的最新行情快照。"""

    response = await okx_client.get_ticker(inst_id)
    return wrap_response(response)
