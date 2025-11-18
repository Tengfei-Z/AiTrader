"""Market data MCP tools."""

import asyncio
from typing import Any

from pydantic import BaseModel, Field

from ...core.okx_client import okx_client
from ..registry import mcp
from .utils import wrap_response

DEFAULT_TICKER_BAR = "3m"
SUPPORTED_TICKER_BARS: dict[str, str] = {"3m": "3m"}


def normalize_bar(value: str) -> str:
    """Validate and normalize the provided OKX bar interval."""

    key = value.strip().lower()
    if not key:
        msg = "bar 参数不能为空字符串"
        raise ValueError(msg)

    normalized = SUPPORTED_TICKER_BARS.get(key)
    if not normalized:
        allowed = ", ".join(sorted(SUPPORTED_TICKER_BARS.values()))
        msg = f"不支持的 bar 周期：{value}。可选值：{allowed}"
        raise ValueError(msg)

    return normalized


@mcp.tool(name="get_ticker")
async def get_ticker(inst_id: str, bar: str | None = None) -> dict[str, Any]:
    """
    获取指定交易对的最新行情快照。

    Parameters:
    - `inst_id`: 交易产品 ID（如 `BTC-USDT-SWAP`），必填，视图获取对应订单簿/价格。
    - `bar`: 兼容保留，但会被忽略并强制使用 `3m`（传入其他值将报错）。

    Example:
    ```json
    {
      "inst_id": "BTC-USDT-SWAP"
    }
    ```
    """

    resolved_bar = normalize_bar(bar) if bar else None
    response = await okx_client.get_ticker(inst_id, bar=resolved_bar)
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


class MultiTickerItem(BaseModel):
    """Single instrument request with optional multiple `bar`s."""

    inst_id: str = Field(..., description="交易产品 ID，例如 BTC-USDT-SWAP")
    bars: list[str] = Field(
        default_factory=lambda: [DEFAULT_TICKER_BAR],
        description="需要返回的 bar 周期，固定为 ['3m']",
        min_length=1,
    )


class MultiTickerPayload(BaseModel):
    """Batch ticker request payload."""

    requests: list[MultiTickerItem] = Field(
        ...,
        min_length=1,
        description="需要查询的合约与周期列表",
    )


@mcp.tool(name="get_multi_ticker")
async def get_multi_ticker(payload: MultiTickerPayload) -> dict[str, Any]:
    """
    批量获取多个合约/周期的最新行情。

    Parameters:
    - `requests`: 数组，每个元素包含 `inst_id` 以及可选的 `bars`（数组）。`bars` 仅允许 `['3m']`；
      如提供其他周期将报错。

    Response:
    ```json
    {
      "results": [
        {"instId": "BTC-USDT-SWAP", "bar": "3m", "ticker": {...}}
      ]
    }
    ```
    """

    async def fetch(inst_id: str, bar_value: str) -> tuple[bool, dict[str, Any] | str]:
        try:
            response = await okx_client.get_ticker(inst_id, bar=bar_value)
            return True, {"instId": inst_id, "bar": bar_value, "ticker": response}
        except Exception as exc:  # noqa: BLE001 - surface per-request failure
            return False, f"{inst_id}/{bar_value}: {exc}"

    coroutines = []
    for item in payload.requests:
        normalized_bars = [normalize_bar(bar) for bar in item.bars]
        for bar_value in normalized_bars:
            coroutines.append(fetch(item.inst_id, bar_value))

    outcomes = await asyncio.gather(*coroutines) if coroutines else []
    successes = [entry for ok, entry in outcomes if ok]
    failures = [error for ok, error in outcomes if not ok]

    if successes:
        payload: dict[str, Any] = {"results": successes}
        if failures:
            payload["skipped"] = len(failures)
        return wrap_response(payload)

    reason = failures[0] if failures else "no requests submitted"
    raise ValueError(f"批量行情请求全部失败：{reason}")
