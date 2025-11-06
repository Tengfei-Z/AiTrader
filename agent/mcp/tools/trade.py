"""Trading MCP tools."""

from typing import Any

from pydantic import BaseModel, ConfigDict, Field, model_validator

from ...core.okx_client import okx_client
from ..registry import mcp
from .utils import wrap_response


class PlaceOrderInput(BaseModel):
    """标准下单参数，兼容 OKX REST 字段命名。"""

    model_config = ConfigDict(populate_by_name=True, extra="allow")

    inst_id: str = Field(..., alias="instId", description="交易产品 ID，例如 BTC-USDT-SWAP")
    td_mode: str = Field(..., alias="tdMode", description="交易模式，cross/isolated/cash")
    side: str = Field(..., description="买卖方向，buy/sell")
    ord_type: str = Field(..., alias="ordType", description="订单类型，limit/market 等")
    size: str = Field(..., alias="sz", description="下单数量")
    px: str | None = Field(None, alias="px", description="限价单价格")
    client_order_id: str | None = Field(None, alias="clOrdId", description="自定义订单 ID")
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


class OrderHistoryQuery(BaseModel):
    """查询历史订单的筛选条件。"""

    model_config = ConfigDict(populate_by_name=True, extra="allow")

    inst_type: str | None = Field(None, alias="instType", description="产品类型，如 SWAP/SPOT")
    inst_id: str | None = Field(None, alias="instId", description="交易产品 ID")
    state: str | None = Field(None, alias="state", description="订单状态过滤，例如 filled/canceled")
    limit: int | None = Field(None, alias="limit", ge=1, le=100, description="返回条数，1-100")


@mcp.tool(name="place_order")
async def place_order_tool(order: PlaceOrderInput) -> dict[str, Any]:
    """提交交易订单。"""

    payload = order.model_dump(by_alias=True, exclude_none=True)
    response = await okx_client.place_order(payload)
    return wrap_response(response)


@mcp.tool(name="cancel_order")
async def cancel_order_tool(order: CancelOrderInput) -> dict[str, Any]:
    """撤销交易订单。"""

    payload = order.model_dump(by_alias=True, exclude_none=True)
    response = await okx_client.cancel_order(
        inst_id=payload["instId"],
        order_id=payload.get("ordId"),
        client_order_id=payload.get("clOrdId"),
    )
    return wrap_response(response)


@mcp.tool()
async def get_order_history(query: OrderHistoryQuery | None = None) -> dict[str, Any]:
    """查询历史订单记录。"""

    query = query or OrderHistoryQuery()
    payload = query.model_dump(by_alias=True, exclude_none=True)
    response = await okx_client.get_order_history(
        inst_type=payload.get("instType"),
        inst_id=payload.get("instId"),
        state=payload.get("state"),
        limit=payload.get("limit"),
    )
    return wrap_response(response)
