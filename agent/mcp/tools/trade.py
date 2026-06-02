"""Trading MCP tools."""

from __future__ import annotations

import asyncio
from decimal import Decimal, InvalidOperation, ROUND_DOWN
from typing import Any, Literal

from pydantic import BaseModel, ConfigDict, Field, model_validator

from ...core.exceptions import ExternalServiceError
from ...core.okx_client import okx_client
from ..registry import mcp
from .utils import wrap_external_error, wrap_response

ORDER_MARGIN_BUFFER = Decimal("0.95")
DECIMAL_ZERO = Decimal("0")
OPPONENT_MAX_CONFIRM_ATTEMPTS = 3
OPPONENT_CONFIRM_DELAY_SECONDS = 1.0
OPPONENT_STATUS_CHECKS = 3
OPPONENT_MARKET_FALLBACK = True


class PlaceOrderInput(BaseModel):
    """标准下单参数，兼容 OKX REST 字段命名。"""

    model_config = ConfigDict(populate_by_name=True, extra="allow")

    inst_id: str = Field(..., alias="instId", description="交易产品 ID，例如 BTC-USDT-SWAP")
    td_mode: str = Field("cross", alias="tdMode", description="交易模式，固定使用全仓 cross")
    side: Literal["buy", "sell"] = Field(..., description="买卖方向，buy/sell")
    pos_side: Literal["long", "short", "net"] | None = Field(
        None,
        alias="posSide",
        description="持仓方向，当前策略固定使用全仓 net",
    )
    ord_type: str = Field(
        ...,
        alias="ordType",
        description=(
            "订单类型：limit/market/opponent。opponent 表示对手价，工具会自动查盘口并转换为限价单；"
            "若短时间未成交，会撤单按最新盘口重试，最后可降级市价成交剩余数量。"
        ),
    )
    size: str = Field(..., alias="sz", description="下单数量")
    px: str | None = Field(None, alias="px", description="限价单价格，仅限价单填写")
    lever: str | None = Field(None, alias="lever", description="杠杆倍数（例如 5 表示 5x）")
    client_order_id: str | None = Field(None, alias="clOrdId", description="自定义订单 ID")
    attach_algo_orders: list[dict[str, Any]] | None = Field(
        None,
        alias="attachAlgoOrds",
        description=(
            "附加止盈/止损数组。OKX 要求使用 tpTriggerPx/tpOrdPx 或 "
            "slTriggerPx/slOrdPx；工具会兼容常见 triggerPx/px 写法并自动转换。"
        ),
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

    LLM 调用时只需明确基础字段（`instId`/`side`/`sz`），工具会强制使用全仓 `tdMode=cross` 与 `posSide=net`；
    `sz` 应按当前本金 all-in 规则由账户可用 USDT、杠杆、价格和合约规格计算，平仓/减仓单除外。
    若需附加止盈止损，可自行传入 `attachAlgoOrds`，优先使用 `tpTriggerPx`/`tpOrdPx` 或 `slTriggerPx`/`slOrdPx`；
    工具也会兼容模型常见的 `triggerPx`/`px` 写法并自动转换。
    `ordType=opponent` 表示对手价：buy 使用卖一价，sell 使用买一价，工具会转换为 OKX `limit` 单提交；
    若短时间未成交，工具会撤单按最新盘口重试，最后可降级市价成交剩余数量，并在返回中给出 `aitraderConfirmation`。
    自定义杠杆倍数时，额外提供 `lever`（例如 `10` 代表 10x），工具会在下单前调用 `/account/set-leverage` 保证目标倍数生效；不提供则沿用 OKX 账户当前设置。

    示例：
    ```json
    {
      "order": {
        "instId": "BTC-USDT-SWAP",
        "tdMode": "cross",
        "side": "buy",
        "posSide": "net",
        "ordType": "market",
        "sz": "0.1",
        "lever": "5"
      }
    }
    ```
    """

    payload = order.model_dump(by_alias=True, exclude_none=True)
    payload = await _prepare_order_payload(payload)

    if order.lever:
        try:
            await okx_client.set_leverage(
                inst_id=order.inst_id,
                lever=order.lever,
                mgn_mode="cross",
                pos_side=None,
            )
        except ExternalServiceError as exc:
            return wrap_external_error("place_order", exc)

    try:
        if _is_prepared_opponent_order(payload):
            response = await _place_opponent_order_with_confirmation(payload)
        else:
            response = await okx_client.place_order(payload)
    except ExternalServiceError as exc:
        return wrap_external_error("place_order", exc)
    return wrap_response(response)


async def _prepare_order_payload(payload: dict[str, Any]) -> dict[str, Any]:
    """Normalize model-produced order parameters before sending them to OKX."""

    prepared = dict(payload)

    # 策略测试固定使用 OKX 全仓净持仓模式；忽略模型传入的 isolated/long/short。
    prepared["tdMode"] = "cross"
    prepared["posSide"] = "net"

    if _is_opponent_order(prepared):
        await _apply_opponent_price(prepared)

    if isinstance(prepared.get("attachAlgoOrds"), list):
        prepared["attachAlgoOrds"] = _normalize_attach_algo_orders(prepared)

    if not _is_reduce_only(prepared):
        await _cap_open_order_size(prepared)

    return prepared


def _is_opponent_order(payload: dict[str, Any]) -> bool:
    order_type = str(payload.get("ordType", "")).strip().lower()
    return order_type in {"opponent", "counterparty", "counter_party", "best_opponent"}


async def _apply_opponent_price(payload: dict[str, Any]) -> None:
    inst_id = str(payload.get("instId", "")).strip()
    side = str(payload.get("side", "")).strip().lower()
    if not inst_id:
        raise ExternalServiceError("opponent order requires instId")

    price = await _fetch_opponent_price(inst_id, side)
    payload["_aitraderOriginalOrdType"] = "opponent"
    payload["ordType"] = "limit"
    payload["px"] = _format_decimal(price)


def _is_prepared_opponent_order(payload: dict[str, Any]) -> bool:
    return str(payload.get("_aitraderOriginalOrdType", "")).lower() == "opponent"


async def _place_opponent_order_with_confirmation(payload: dict[str, Any]) -> dict[str, Any]:
    base_payload = _okx_order_payload(payload)
    remaining_size = _to_decimal(base_payload.get("sz"))
    if remaining_size is None or remaining_size <= DECIMAL_ZERO:
        return await okx_client.place_order(base_payload)

    attempts: list[dict[str, Any]] = []
    last_response: dict[str, Any] | None = None

    for attempt in range(1, OPPONENT_MAX_CONFIRM_ATTEMPTS + 1):
        attempt_payload = dict(base_payload)
        _set_order_size(attempt_payload, remaining_size)
        await _apply_opponent_price(attempt_payload)
        attempt_payload = _okx_order_payload(attempt_payload)

        order_response = await okx_client.place_order(attempt_payload)
        last_response = order_response if isinstance(order_response, dict) else {"data": order_response}
        order_entry = _first_response_entry(order_response)
        ord_id = _extract_ord_id(order_entry)
        if not ord_id:
            attempts.append(
                {
                    "attempt": attempt,
                    "action": "place_opponent_limit",
                    "status": "accepted_without_ord_id",
                    "payload": attempt_payload,
                    "response": order_response,
                }
            )
            break

        status = await _confirm_order_status(
            inst_id=str(attempt_payload["instId"]),
            ord_id=ord_id,
        )
        filled_size = _to_decimal(status.get("accFillSz")) or DECIMAL_ZERO
        state = str(status.get("state") or "").lower()
        attempts.append(
            {
                "attempt": attempt,
                "action": "place_opponent_limit",
                "ordId": ord_id,
                "state": state or "unknown",
                "requestedSz": attempt_payload.get("sz"),
                "filledSz": _format_decimal(filled_size),
                "px": attempt_payload.get("px"),
            }
        )

        if state == "filled":
            return _with_opponent_confirmation(last_response, attempts, "filled")

        remaining_size = max(DECIMAL_ZERO, remaining_size - filled_size)
        if remaining_size <= DECIMAL_ZERO:
            return _with_opponent_confirmation(last_response, attempts, "filled")

        if state not in {"filled", "canceled"}:
            cancel_response = await okx_client.cancel_order(
                inst_id=str(attempt_payload["instId"]),
                order_id=ord_id,
            )
            attempts[-1]["cancelResponse"] = cancel_response
            refreshed_status = await _confirm_order_status(
                inst_id=str(attempt_payload["instId"]),
                ord_id=ord_id,
            )
            refreshed_filled_size = _to_decimal(refreshed_status.get("accFillSz"))
            if refreshed_filled_size is not None and refreshed_filled_size > filled_size:
                remaining_size = max(
                    DECIMAL_ZERO,
                    remaining_size - (refreshed_filled_size - filled_size),
                )
                attempts[-1]["stateAfterCancel"] = str(
                    refreshed_status.get("state") or "unknown"
                ).lower()
                attempts[-1]["filledSz"] = _format_decimal(refreshed_filled_size)
            if remaining_size <= DECIMAL_ZERO:
                return _with_opponent_confirmation(last_response, attempts, "filled")

    if OPPONENT_MARKET_FALLBACK and remaining_size > DECIMAL_ZERO:
        market_payload = dict(base_payload)
        market_payload["ordType"] = "market"
        market_payload.pop("px", None)
        _set_order_size(market_payload, remaining_size)
        market_payload = _okx_order_payload(market_payload)

        market_response = await okx_client.place_order(market_payload)
        last_response = market_response if isinstance(market_response, dict) else {"data": market_response}
        market_entry = _first_response_entry(market_response)
        market_ord_id = _extract_ord_id(market_entry)
        market_status: dict[str, Any] = {}
        if market_ord_id:
            market_status = await _confirm_order_status(
                inst_id=str(market_payload["instId"]),
                ord_id=market_ord_id,
            )
        attempts.append(
            {
                "attempt": len(attempts) + 1,
                "action": "market_fallback",
                "ordId": market_ord_id,
                "state": str(market_status.get("state") or "accepted").lower(),
                "requestedSz": market_payload.get("sz"),
                "filledSz": market_status.get("accFillSz"),
            }
        )
        final_state = str(market_status.get("state") or "").lower()
        return _with_opponent_confirmation(
            last_response,
            attempts,
            "filled" if final_state == "filled" else "market_submitted",
        )

    return _with_opponent_confirmation(last_response or {"code": "0", "data": []}, attempts, "not_filled")


async def _confirm_order_status(inst_id: str, ord_id: str) -> dict[str, Any]:
    for _ in range(OPPONENT_STATUS_CHECKS):
        await asyncio.sleep(OPPONENT_CONFIRM_DELAY_SECONDS)

        details = await okx_client.get_order_details(inst_id=inst_id, order_id=ord_id)
        details_entry = _find_order_entry(details, ord_id)
        if details_entry:
            return details_entry

        pending = await okx_client.get_open_orders(
            inst_type="SWAP",
            inst_id=inst_id,
            order_id=ord_id,
        )
        pending_entry = _find_order_entry(pending, ord_id)
        if pending_entry:
            return pending_entry

        history = await okx_client.get_order_history(
            inst_type="SWAP",
            inst_id=inst_id,
            order_id=ord_id,
            limit=20,
        )
        history_entry = _find_order_entry(history, ord_id)
        if history_entry:
            return history_entry

    return {"ordId": ord_id, "state": "unknown"}


def _find_order_entry(response: Any, ord_id: str) -> dict[str, Any] | None:
    for entry in _response_data(response):
        if isinstance(entry, dict) and str(entry.get("ordId") or "") == ord_id:
            return entry
    return None


def _first_response_entry(response: Any) -> dict[str, Any]:
    for entry in _response_data(response):
        if isinstance(entry, dict):
            return entry
    return {}


def _extract_ord_id(entry: dict[str, Any]) -> str | None:
    ord_id = entry.get("ordId") or entry.get("orderId") or entry.get("order_id")
    return str(ord_id) if ord_id else None


def _set_order_size(payload: dict[str, Any], size: Decimal) -> None:
    formatted = _format_decimal(size)
    payload["sz"] = formatted
    for entry in payload.get("attachAlgoOrds") or []:
        if isinstance(entry, dict) and "sz" in entry:
            entry["sz"] = formatted


def _okx_order_payload(payload: dict[str, Any]) -> dict[str, Any]:
    return {key: value for key, value in payload.items() if not str(key).startswith("_aitrader")}


def _with_opponent_confirmation(
    response: dict[str, Any],
    attempts: list[dict[str, Any]],
    status: str,
) -> dict[str, Any]:
    enriched = dict(response)
    enriched["aitraderConfirmation"] = {
        "type": "opponent_order_retry",
        "status": status,
        "attempts": attempts,
    }
    return enriched


async def _fetch_opponent_price(inst_id: str, side: str) -> Decimal:
    try:
        response = await okx_client.get_order_book(inst_id, depth=1)
    except ExternalServiceError:
        raise
    except Exception as exc:
        raise ExternalServiceError(f"failed to fetch order book for opponent order: {exc}") from exc

    books = _response_data(response)
    if not books or not isinstance(books[0], dict):
        raise ExternalServiceError(f"empty order book for opponent order: {inst_id}")

    if side == "buy":
        levels = books[0].get("asks") or []
        level_name = "ask"
    elif side == "sell":
        levels = books[0].get("bids") or []
        level_name = "bid"
    else:
        raise ExternalServiceError(f"unsupported side for opponent order: {side}")

    if not levels or not isinstance(levels[0], (list, tuple)) or not levels[0]:
        raise ExternalServiceError(f"missing {level_name} price for opponent order: {inst_id}")

    price = _to_decimal(levels[0][0])
    if price is None or price <= DECIMAL_ZERO:
        raise ExternalServiceError(f"invalid {level_name} price for opponent order: {levels[0][0]}")

    return price


def _normalize_attach_algo_orders(order_payload: dict[str, Any]) -> list[dict[str, Any]]:
    normalized_orders: list[dict[str, Any]] = []
    for raw_entry in order_payload.get("attachAlgoOrds") or []:
        if not isinstance(raw_entry, dict):
            continue

        entry = {
            str(key): value
            for key, value in raw_entry.items()
            if value is not None and value != ""
        }
        if not entry:
            continue

        if "algoClOrdId" in entry and "attachAlgoClOrdId" not in entry:
            entry["attachAlgoClOrdId"] = entry.pop("algoClOrdId")

        _normalize_trigger_price_type(entry)
        _fill_missing_attached_order_prices(entry)
        _convert_generic_trigger_fields(entry, order_payload)
        _drop_non_attach_algo_fields(entry)

        if _has_attached_trigger(entry):
            normalized_orders.append(entry)

    return normalized_orders


def _normalize_trigger_price_type(entry: dict[str, Any]) -> None:
    trigger_kind = entry.pop("triggerPxType", None)
    tp_kind = entry.pop("tpTriggerKind", None)
    sl_kind = entry.pop("slTriggerKind", None)

    if trigger_kind:
        entry.setdefault("tpTriggerPxType", trigger_kind)
        entry.setdefault("slTriggerPxType", trigger_kind)
    if tp_kind:
        entry.setdefault("tpTriggerPxType", tp_kind)
    if sl_kind:
        entry.setdefault("slTriggerPxType", sl_kind)

    if "tpTriggerPx" in entry:
        entry.setdefault("tpTriggerPxType", "last")
    if "slTriggerPx" in entry:
        entry.setdefault("slTriggerPxType", "last")


def _fill_missing_attached_order_prices(entry: dict[str, Any]) -> None:
    px = entry.get("px")
    if "tpTriggerPx" in entry:
        entry.setdefault("tpOrdPx", px or entry["tpTriggerPx"])
        entry.setdefault("tpTriggerPxType", "last")
    if "slTriggerPx" in entry:
        entry.setdefault("slOrdPx", px or entry["slTriggerPx"])
        entry.setdefault("slTriggerPxType", "last")


def _convert_generic_trigger_fields(
    entry: dict[str, Any],
    order_payload: dict[str, Any],
) -> None:
    if "triggerPx" not in entry or _has_attached_trigger(entry):
        entry.pop("triggerPx", None)
        entry.pop("px", None)
        return

    trigger_px = entry.pop("triggerPx")
    order_px = entry.pop("px", trigger_px)
    trigger_kind = entry.get("tpTriggerPxType") or entry.get("slTriggerPxType") or "last"
    target = _infer_attached_algo_target(entry, order_payload, trigger_px)

    if target == "tp":
        entry["tpTriggerPx"] = trigger_px
        entry["tpOrdPx"] = order_px
        entry["tpTriggerPxType"] = trigger_kind
        entry.pop("slTriggerPxType", None)
    else:
        entry["slTriggerPx"] = trigger_px
        entry["slOrdPx"] = order_px
        entry["slTriggerPxType"] = trigger_kind
        entry.pop("tpTriggerPxType", None)


def _infer_attached_algo_target(
    entry: dict[str, Any],
    order_payload: dict[str, Any],
    trigger_px: Any,
) -> Literal["tp", "sl"]:
    label = " ".join(
        str(entry.get(key, "")).lower()
        for key in ("attachAlgoClOrdId", "algoOrdType", "tag", "name")
    )
    if any(marker in label for marker in ("take", "profit", "tp")):
        return "tp"
    if any(marker in label for marker in ("stop", "loss", "sl")):
        return "sl"

    entry_px = _to_decimal(order_payload.get("px"))
    trigger = _to_decimal(trigger_px)
    side = str(order_payload.get("side", "")).lower()
    if entry_px is not None and trigger is not None:
        if side == "buy":
            return "tp" if trigger > entry_px else "sl"
        if side == "sell":
            return "tp" if trigger < entry_px else "sl"

    return "sl"


def _drop_non_attach_algo_fields(entry: dict[str, Any]) -> None:
    for key in (
        "algoOrdType",
        "algoSide",
        "ordType",
        "reduceOnly",
        "callbackRatio",
        "callbackSpread",
        "activePx",
        "px",
        "triggerPx",
    ):
        entry.pop(key, None)


def _has_attached_trigger(entry: dict[str, Any]) -> bool:
    return "tpTriggerPx" in entry or "slTriggerPx" in entry


async def _cap_open_order_size(payload: dict[str, Any]) -> None:
    requested_size = _to_decimal(payload.get("sz"))
    lever = _to_decimal(payload.get("lever"))
    if requested_size is None or lever is None or requested_size <= DECIMAL_ZERO:
        return

    price = _to_decimal(payload.get("px"))
    if price is None or price <= DECIMAL_ZERO:
        price = await _fetch_latest_price(str(payload.get("instId", "")))
    if price is None or price <= DECIMAL_ZERO:
        return

    instrument = await _fetch_instrument_spec(str(payload.get("instId", "")))
    if not instrument:
        return

    ct_val = _to_decimal(instrument.get("ctVal"))
    ct_mult = _to_decimal(instrument.get("ctMult")) or Decimal("1")
    lot_size = _to_decimal(instrument.get("lotSz")) or Decimal("1")
    min_size = _to_decimal(instrument.get("minSz")) or lot_size
    available = await _fetch_available_usdt()
    if (
        ct_val is None
        or ct_val <= DECIMAL_ZERO
        or lot_size <= DECIMAL_ZERO
        or available is None
        or available <= DECIMAL_ZERO
    ):
        return

    max_size = (available * lever * ORDER_MARGIN_BUFFER) / (price * ct_val * ct_mult)
    capped_size = _floor_to_step(max_size, lot_size)
    if capped_size < min_size:
        return

    if requested_size > capped_size:
        payload["sz"] = _format_decimal(capped_size)
        for entry in payload.get("attachAlgoOrds") or []:
            if isinstance(entry, dict) and "sz" in entry:
                entry["sz"] = payload["sz"]


async def _fetch_latest_price(inst_id: str) -> Decimal | None:
    if not inst_id:
        return None
    try:
        response = await okx_client.get_ticker(inst_id)
    except ExternalServiceError:
        return None
    for entry in _response_data(response):
        if isinstance(entry, dict):
            price = _to_decimal(entry.get("last"))
            if price is not None:
                return price
    return None


async def _fetch_instrument_spec(inst_id: str) -> dict[str, Any] | None:
    if not inst_id:
        return None
    inst_type = inst_id.upper().split("-")[-1] if "-" in inst_id else "SWAP"
    try:
        response = await okx_client.get_instruments(inst_type=inst_type, inst_id=inst_id)
    except ExternalServiceError:
        return None
    for entry in _response_data(response):
        if isinstance(entry, dict):
            return entry
    return None


async def _fetch_available_usdt() -> Decimal | None:
    try:
        response = await okx_client.get_account_balance()
    except ExternalServiceError:
        return None

    for account in _response_data(response):
        if not isinstance(account, dict):
            continue
        for detail in account.get("details") or []:
            if not isinstance(detail, dict) or str(detail.get("ccy", "")).upper() != "USDT":
                continue
            for key in ("availEq", "availBal", "cashBal", "eq"):
                amount = _to_decimal(detail.get(key))
                if amount is not None:
                    return amount
    return None


def _response_data(response: Any) -> list[Any]:
    if isinstance(response, dict):
        data = response.get("data")
        return data if isinstance(data, list) else []
    return response if isinstance(response, list) else []


def _is_reduce_only(payload: dict[str, Any]) -> bool:
    value = payload.get("reduceOnly")
    if isinstance(value, bool):
        return value
    return str(value).lower() == "true"


def _floor_to_step(value: Decimal, step: Decimal) -> Decimal:
    if step <= DECIMAL_ZERO:
        return value
    return (value / step).to_integral_value(rounding=ROUND_DOWN) * step


def _to_decimal(value: Any) -> Decimal | None:
    if value is None or value == "":
        return None
    try:
        return Decimal(str(value))
    except (InvalidOperation, ValueError):
        return None


def _format_decimal(value: Decimal) -> str:
    normalized = value.normalize()
    return format(normalized, "f")


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
    try:
        response = await okx_client.cancel_order(
            inst_id=payload["instId"],
            order_id=payload.get("ordId"),
            client_order_id=payload.get("clOrdId"),
        )
    except ExternalServiceError as exc:
        return wrap_external_error("cancel_order", exc)
    return wrap_response(response)
