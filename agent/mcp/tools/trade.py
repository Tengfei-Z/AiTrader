"""Trading MCP tools."""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator

from ...core.okx_client import okx_client
from ..registry import mcp
from .utils import wrap_response


class PlaceOrderInput(BaseModel):
    """标准下单参数，兼容 OKX REST 字段命名。"""

    model_config = ConfigDict(populate_by_name=True, extra="allow")

    inst_id: str = Field(..., alias="instId", description="交易产品 ID，例如 BTC-USDT-SWAP")
    td_mode: str = Field(..., alias="tdMode", description="交易模式，cross/isolated/cash")
    side: Literal["buy", "sell"] = Field(..., description="买卖方向，buy/sell")
    pos_side: Literal["long", "short"] | None = Field(
        None,
        alias="posSide",
        description="持仓方向（long/short），SWAP 合约必填",
    )
    ord_type: str = Field(..., alias="ordType", description="订单类型，limit/market 等")
    size: str = Field(..., alias="sz", description="下单数量")
    px: str | None = Field(None, alias="px", description="限价单价格，仅限价单填写")
    lever: str | None = Field(None, alias="lever", description="杠杆倍数（例如 5 表示 5x）")
    client_order_id: str | None = Field(None, alias="clOrdId", description="自定义订单 ID")
    attach_algo_orders: list[dict[str, Any]] | None = Field(
        None,
        alias="attachAlgoOrds",
        description="附加算法单数组（例如止盈/止损），可选，需手动提供完整字段。",
    )
    reduce_only: bool | None = Field(None, alias="reduceOnly", description="是否只减仓")

class CancelOrderInput(BaseModel):
    """撤单参数，至少提供 OKX 订单 ID 或客户端订单 ID。"""

    model_config = ConfigDict(populate_by_name=True)

    inst_id: str = Field(..., alias="instId", description="交易产品 ID")
    order_id: str | None = Field(None, alias="ordId", description="OKX 订单 ID")
    client_order_id: str | None = Field(None, alias="clOrdId", description="客户端自定义订单 ID")

    @model_validator(mode="after")
    def ensure_identifier(self) -> "CancelOrderInput":
        if not self.order_id and not self.client_order_id:
            msg = "必须提供 ordId 或 clOrdId 至少一个用于撤单。"
            raise ValueError(msg)
        return self


@mcp.tool(name="place_order")
async def place_order_tool(order: PlaceOrderInput) -> dict[str, Any]:
    """
    提交交易订单。

    LLM 调用时只需明确基础字段（`instId`/`tdMode`/`side`/`posSide`/`sz`），止盈止损目前非必需；
    若需附加算法单，可自行传入 `attachAlgoOrds`，每个条目需自备 `algoSide`/`algoOrdType`/`triggerPx`/`px` 等字段。
    自定义杠杆倍数时，额外提供 `lever`（例如 `10` 代表 10x），工具会在下单前调用 `/account/set-leverage` 保证目标倍数生效；不提供则沿用 OKX 账户当前设置。

    示例：
    ```json
    {
      "order": {
        "instId": "BTC-USDT-SWAP",
        "tdMode": "cross",
        "side": "buy",
        "posSide": "long",
        "ordType": "market",
        "sz": "0.1",
        "lever": "5"
      }
    }
    ```
    """

    payload = order.model_dump(by_alias=True, exclude_none=True)

    if order.lever:
        td_mode_normalized = order.td_mode.lower()
        if td_mode_normalized in {"cross", "isolated"}:
            await okx_client.set_leverage(
                inst_id=order.inst_id,
                lever=order.lever,
                mgn_mode=td_mode_normalized,
                pos_side=order.pos_side if td_mode_normalized == "isolated" else None,
            )

    response = await okx_client.place_order(payload)
    return wrap_response(response)


@mcp.tool(name="cancel_order")
async def cancel_order_tool(order: CancelOrderInput) -> dict[str, Any]:
    """
    撤销交易订单。

    参数 `instId` 必填，并需要至少提供 `ordId` 或 `clOrdId` 其一用于定位订单；
    `ordId` 优先匹配 OKX 真实订单号，`clOrdId` 可用于追踪自定义 ID。

    示例：
    ```json
    {
      "order": {
        "instId": "BTC-USDT-SWAP",
        "ordId": "1234567890123456"
      }
    }
    ```
    """

    payload = order.model_dump(by_alias=True, exclude_none=True)
    response = await okx_client.cancel_order(
        inst_id=payload["instId"],
        order_id=payload.get("ordId"),
        client_order_id=payload.get("clOrdId"),
    )
    return wrap_response(response)
