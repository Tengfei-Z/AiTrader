"""Trading MCP tools."""

from __future__ import annotations

from decimal import Decimal, InvalidOperation, ROUND_HALF_UP
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator

from ...core.okx_client import okx_client
from ..registry import mcp
from .utils import wrap_response


class PlaceOrderInput(BaseModel):
    """标准简单下单参数，只暴露对模型必须的字段。"""

    model_config = ConfigDict(populate_by_name=True, extra="ignore")

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
    sl_trigger_px: str | None = Field(
        None,
        alias="slTriggerPx",
        description="止损触发价",
    )
    tp_trigger_px: str | None = Field(
        None,
        alias="tpTriggerPx",
        description="止盈触发价",
    )
    client_order_id: str | None = Field(None, alias="clOrdId", description="自定义订单 ID")
    reduce_only: bool | None = Field(None, alias="reduceOnly", description="是否只减仓")

def build_attach_algo_orders(order: PlaceOrderInput) -> list[dict[str, str]]:
    """为下单请求构造 attachAlgoOrds（自动补齐）。"""

    attach: list[dict[str, str]] = []

    def _algo_order(side: Literal["sl", "tp"], trigger: str) -> dict[str, str]:
        return {
            "algoSide": side,
            "algoOrdType": "conditional",
            "triggerPx": trigger,
            "px": trigger,
            "ordType": "limit",
            "sz": order.size,
        }

    if order.sl_trigger_px:
        attach.append(_algo_order("sl", order.sl_trigger_px))
    if order.tp_trigger_px:
        attach.append(_algo_order("tp", order.tp_trigger_px))
    return attach


def _to_decimal(value: Any) -> Decimal | None:
    try:
        return Decimal(str(value))
    except (InvalidOperation, TypeError):
        return None


async def _apply_default_triggers(order: PlaceOrderInput, payload: dict[str, Any]) -> dict[str, Any]:
    if payload.get("slTriggerPx") or payload.get("tpTriggerPx"):
        return payload

    ticker = await okx_client.get_ticker(order.inst_id)
    data = ticker.get("data") if isinstance(ticker, dict) else []
    if not data:
        return payload

    last_decimal = _to_decimal(data[0].get("last"))
    if last_decimal is None:
        return payload

    step = Decimal(1).scaleb(last_decimal.as_tuple().exponent)
    buy_factors = (Decimal("0.985"), Decimal("1.015"))
    sell_factors = (Decimal("1.015"), Decimal("0.985"))
    sl_factor, tp_factor = buy_factors if order.side == "buy" else sell_factors

    sl = (last_decimal * sl_factor).quantize(step, rounding=ROUND_HALF_UP)
    tp = (last_decimal * tp_factor).quantize(step, rounding=ROUND_HALF_UP)

    payload["slTriggerPx"] = format(sl, "f")
    payload["tpTriggerPx"] = format(tp, "f")
    return payload

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


class OrderHistoryQuery(BaseModel):
    """查询历史订单的筛选条件。"""

    model_config = ConfigDict(populate_by_name=True, extra="allow")

    inst_type: str | None = Field(None, alias="instType", description="产品类型，如 SWAP/SPOT")
    inst_id: str | None = Field(None, alias="instId", description="交易产品 ID")
    state: str | None = Field(None, alias="state", description="订单状态过滤，例如 filled/canceled")
    limit: int | None = Field(None, alias="limit", ge=1, le=100, description="返回条数，1-100")


@mcp.tool(name="place_order")
async def place_order_tool(order: PlaceOrderInput) -> dict[str, Any]:
    """
    提交交易订单。

    LLM 调用时只需明确基础字段（`instId`/`tdMode`/`side`/`posSide`/`sz`）以及止盈止损触发价。
    工具内部会自动为这些触发价生成 `attachAlgoOrds`，无需模型手动拼装。

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
        "slTriggerPx": "103000",
        "tpTriggerPx": "108000"
      }
    }
    ```
    """

    payload = order.model_dump(by_alias=True, exclude_none=True)
    payload = await _apply_default_triggers(order, payload)
    attach = build_attach_algo_orders(order)
    if attach:
        payload["attachAlgoOrds"] = attach
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


@mcp.tool()
async def get_order_history(query: OrderHistoryQuery | None = None) -> dict[str, Any]:
    """
    查询历史订单记录。

    可选 `instType`/`instId`/`state`/`limit` 过滤：若不提供参数将返回最近的订单；
    `state` 可以是 `filled`/`canceled` 等，`limit` 限制条数（默认 100，最大 100）。

    示例：
    ```json
    {
      "query": {
        "instType": "SWAP",
        "limit": 10
      }
    }
    ```
    """

    query = query or OrderHistoryQuery()
    payload = query.model_dump(by_alias=True, exclude_none=True)
    response = await okx_client.get_order_history(
        inst_type=payload.get("instType"),
        inst_id=payload.get("instId"),
        state=payload.get("state"),
        limit=payload.get("limit"),
    )
    return wrap_response(response)
