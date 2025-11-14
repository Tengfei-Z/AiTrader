"""Market data MCP tools."""

from typing import Any

from pydantic import BaseModel, Field

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


class InstrumentSpecRequest(BaseModel):
    """Parameters for fetching OKX instrument specifications."""

    inst_id: str = Field(..., alias="instId", description="交易产品 ID，例如 BTC-USDT-SWAP")
    inst_type: str | None = Field(
        None,
        alias="instType",
        description="产品类型（如 SWAP/SPOT/FUTURES），默认根据 instId 自动推断",
    )

    def resolve_inst_type(self) -> str:
        if self.inst_type:
            return self.inst_type
        parts = self.inst_id.upper().split("-")
        if len(parts) >= 3:
            return parts[-1]
        msg = "无法根据 instId 推断 instType，请显式提供 instType 参数。"
        raise ValueError(msg)


@mcp.tool(name="get_instrument_specs")
async def get_instrument_specs(query: InstrumentSpecRequest) -> dict[str, Any]:
    """
    查询合约规格（最小交易单位、面值、tick 等）。

    Parameters:
    - `instId`: 交易产品 ID，必填。
    - `instType`: 产品类型，默认根据 `instId` 的结尾推断（例如 `BTC-USDT-SWAP` → `SWAP`）。
    """

    inst_type = query.resolve_inst_type()
    response = await okx_client.get_instruments(
        inst_type=inst_type,
        inst_id=query.inst_id,
    )
    return wrap_response(response)
