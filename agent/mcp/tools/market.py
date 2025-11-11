"""Market data MCP tools."""

from typing import Any

from ...core.okx_client import okx_client
from ..registry import mcp
from .utils import wrap_response


@mcp.tool(name="get_ticker")
async def get_ticker(inst_id: str) -> dict[str, Any]:
    """
    获取指定交易对的最新行情快照。

    Parameters:
    - `inst_id`: 交易产品 ID（如 `BTC-USDT-SWAP`），必填，视图获取对应订单簿/价格。

    Example:
    ```json
    {
      "inst_id": "BTC-USDT-SWAP"
    }
    ```
    """

    response = await okx_client.get_ticker(inst_id)
    return wrap_response(response)
