"""Async OKX REST client."""

import asyncio
import base64
import hashlib
import hmac
import json
from datetime import datetime, timezone
from typing import Any

import httpx

from .config import get_settings
from .exceptions import ExternalServiceError, RateLimitExceeded
from .http_client import async_http_client
from .logging_config import get_logger


class OKXClient:
    """Minimal async client for OKX REST API."""

    DEFAULT_TICKER_BAR = "3m"
    INDICATOR_CANDLE_LIMIT = 60

    def __init__(self) -> None:
        self._settings = get_settings()

    async def _request(
        self,
        method: str,
        path: str,
        *,
        params: dict[str, Any] | None = None,
        body: dict[str, Any] | None = None,
        auth: bool = True,
    ) -> Any:
        timestamp = datetime.now(tz=timezone.utc).isoformat(timespec="milliseconds").replace(
            "+00:00", "Z"
        )
        request_body = json.dumps(body) if body else ""
        query_string = ""
        request_params = None
        if params:
            request_params = httpx.QueryParams(params)
            query_string = f"?{request_params}"
        request_path = f"{path}{query_string}"

        headers: dict[str, str] = {
            "Content-Type": "application/json",
            "OK-ACCESS-PASSPHRASE": self._settings.okx_passphrase.get_secret_value(),
            "OK-ACCESS-TIMESTAMP": timestamp,
        }

        if self._settings.okx_use_simulated:
            headers["x-simulated-trading"] = "1"

        if auth:
            sign_payload = f"{timestamp}{method.upper()}{request_path}{request_body}".encode("utf-8")
            signature = base64.b64encode(
                hmac.new(
                    self._settings.okx_secret_key.get_secret_value().encode("utf-8"),
                    sign_payload,
                    hashlib.sha256,
                ).digest()
            ).decode("utf-8")

            headers.update(
                {
                    "OK-ACCESS-KEY": self._settings.okx_api_key.get_secret_value(),
                    "OK-ACCESS-SIGN": signature,
                }
            )

        max_retries = max(0, int(self._settings.okx_http_max_retries))
        base_delay = max(0.0, float(self._settings.okx_http_retry_backoff))
        total_attempts = max_retries + 1
        last_error: Exception | None = None

        for attempt in range(1, total_attempts + 1):
            try:
                async with async_http_client(
                    base_url=str(self._settings.okx_base_url), timeout=15.0
                ) as client:
                    response = await client.request(
                        method=method.upper(),
                        url=path,
                        params=request_params,
                        content=request_body if request_body else None,
                        headers=headers,
                    )
                break
            except RETRYABLE_EXCEPTIONS as exc:
                last_error = exc
                if attempt >= total_attempts:
                    raise ExternalServiceError(
                        f"OKX request failed after {max_retries} retries: {exc}"
                    ) from exc
                delay = base_delay * (2 ** (attempt - 1)) if base_delay else 0.0
                logger.warning(
                    "okx_http_retry",
                    method=method.upper(),
                    path=path,
                    attempt=attempt,
                    max_retries=max_retries,
                    delay=delay,
                    error=str(exc),
                )
                if delay > 0:
                    await asyncio.sleep(delay)
        else:
            raise ExternalServiceError(
                f"OKX request failed without response: {last_error}"
            ) from last_error

        if response.status_code == httpx.codes.TOO_MANY_REQUESTS:
            raise RateLimitExceeded(response.text)

        if response.is_error:
            raise ExternalServiceError(
                f"OKX responded with {response.status_code}: {response.text}"
            )

        payload = response.json()
        if isinstance(payload, dict) and payload.get("code") not in ("0", 0):
            raise ExternalServiceError(f"OKX business error: {payload}")

        return payload

    async def get_ticker(self, inst_id: str, *, bar: str | None = None) -> Any:
        """Fetch the latest candle snapshot and project it as a ticker."""

        resolved_bar = self.DEFAULT_TICKER_BAR
        payload = await self._request(
            "GET",
            "/api/v5/market/candles",
            params={
                "instId": inst_id,
                "bar": resolved_bar,
                "limit": self.INDICATOR_CANDLE_LIMIT,
            },
            auth=False,
        )

        raw_data: list[Any] = []
        if isinstance(payload, dict):
            raw_data = payload.get("data") or []
        elif isinstance(payload, list):
            raw_data = payload

        if not raw_data:
            raise ExternalServiceError(
                f"OKX candles returned empty data for {inst_id} ({resolved_bar})"
            )

        candle = raw_data[0]
        if not isinstance(candle, (list, tuple)) or len(candle) < 5:
            raise ExternalServiceError(f"Unexpected candle payload: {candle!r}")

        def get_value(index: int) -> str | None:
            try:
                value = candle[index]
            except IndexError:
                return None
            return str(value) if value is not None else None

        ts = get_value(0) or datetime.now(tz=timezone.utc).isoformat()
        open_px = get_value(1)
        high_px = get_value(2)
        low_px = get_value(3)
        close_px = get_value(4)
        volume = get_value(5)
        volume_ccy = get_value(6)
        volume_ccy_quote = get_value(7)
        confirm = get_value(8)

        inst_type = inst_id.split("-")[-1] if "-" in inst_id else "SWAP"
        ticker_snapshot = {
            "instType": inst_type,
            "instId": inst_id,
            "bar": resolved_bar,
            "source": "candles",
            "ts": ts,
            "last": close_px,
            "lastSz": None,
            "open24h": open_px,
            "open": open_px,
            "high24h": high_px,
            "high": high_px,
            "low24h": low_px,
            "low": low_px,
            "vol24h": volume,
            "vol": volume,
            "volCcy24h": volume_ccy,
            "volCcy": volume_ccy,
            "volCcyQuote": volume_ccy_quote,
            "confirm": confirm,
            "bidPx": None,
            "askPx": None,
            "bidSz": None,
            "askSz": None,
        }

        indicators = build_indicator_payload(raw_data)
        if indicators:
            ticker_snapshot.update(indicators)

        if isinstance(payload, dict):
            return {
                "code": payload.get("code", "0"),
                "msg": payload.get("msg", ""),
                "data": [ticker_snapshot],
            }
        return {"code": "0", "msg": "", "data": [ticker_snapshot]}

    async def get_order_book(self, inst_id: str, depth: int = 5) -> Any:
        """Fetch order book depth."""

        return await self._request(
            "GET",
            "/api/v5/market/books",
            params={"instId": inst_id, "sz": depth},
            auth=False,
        )

    async def get_candles(self, inst_id: str, bar: str = "1m", limit: int = 100) -> Any:
        """Fetch historical candles."""

        return await self._request(
            "GET",
            "/api/v5/market/candles",
            params={"instId": inst_id, "bar": bar, "limit": limit},
            auth=False,
        )

    async def get_instruments(
        self,
        *,
        inst_type: str,
        inst_id: str | None = None,
        underlying: str | None = None,
    ) -> Any:
        """Fetch instrument specifications such as lot size and tick size."""

        params: dict[str, Any] = {"instType": inst_type}
        if inst_id:
            params["instId"] = inst_id
        if underlying:
            params["uly"] = underlying

        return await self._request(
            "GET",
            "/api/v5/public/instruments",
            params=params,
            auth=False,
        )

    async def get_account_balance(self) -> Any:
        """Fetch account balance."""

        return await self._request("GET", "/api/v5/account/balance")

    async def get_positions(
        self,
        inst_type: str | None = None,
        inst_id: str | None = None,
    ) -> Any:
        """Fetch current positions."""

        params: dict[str, Any] = {}
        if inst_type:
            params["instType"] = inst_type
        if inst_id:
            params["instId"] = inst_id

        if not params:
            params = None
        return await self._request("GET", "/api/v5/account/positions", params=params)

    async def get_open_orders(self, inst_type: str | None = None) -> Any:
        """Fetch open orders."""

        params = {"instType": inst_type} if inst_type else None
        return await self._request("GET", "/api/v5/trade/orders-pending", params=params)

    async def get_order_history(
        self,
        *,
        inst_type: str | None = None,
        inst_id: str | None = None,
        state: str | None = None,
        limit: int | None = None,
    ) -> Any:
        """Fetch historical orders."""

        params: dict[str, Any] = {}
        if inst_type:
            params["instType"] = inst_type
        if inst_id:
            params["instId"] = inst_id
        if state:
            params["state"] = state
        if limit:
            params["limit"] = limit

        return await self._request("GET", "/api/v5/trade/orders-history", params=params or None)

    async def place_order(self, order: dict[str, Any]) -> Any:
        """Submit an order payload to OKX."""

        return await self._request("POST", "/api/v5/trade/order", body=order)

    async def set_leverage(
        self,
        *,
        lever: str,
        mgn_mode: str,
        inst_id: str | None = None,
        ccy: str | None = None,
        pos_side: str | None = None,
    ) -> Any:
        """Configure leverage for a contract or margin currency."""

        payload: dict[str, Any] = {
            "lever": lever,
            "mgnMode": mgn_mode,
        }
        if inst_id:
            payload["instId"] = inst_id
        if ccy:
            payload["ccy"] = ccy
        if pos_side:
            payload["posSide"] = pos_side

        return await self._request("POST", "/api/v5/account/set-leverage", body=payload)

    async def cancel_order(
        self,
        *,
        inst_id: str,
        order_id: str | None = None,
        client_order_id: str | None = None,
    ) -> Any:
        """Cancel an existing order."""

        if not order_id and not client_order_id:
            raise ValueError("Either order_id or client_order_id must be provided.")

        payload = {
            "instId": inst_id,
        }
        if order_id:
            payload["ordId"] = order_id
        if client_order_id:
            payload["clOrdId"] = client_order_id

        return await self._request("POST", "/api/v5/trade/cancel-order", body=payload)


def build_indicator_payload(raw_candles: list[Any]) -> dict[str, Any]:
    """Compute EMA/MACD/RSI series from OKX candles."""

    if not raw_candles:
        return {}

    ordered_timestamps, close_series = _extract_ordered_series(raw_candles)
    if not close_series:
        return {}

    ema20_series = calculate_ema_series(close_series, 20)
    macd_dif_series, macd_signal_series, macd_hist_series = calculate_macd_series(close_series)
    rsi7_series = calculate_rsi_series(close_series, 7)

    def format_value(value: float | None) -> str | None:
        if value is None:
            return None
        return f"{value:.8f}".rstrip("0").rstrip(".")

    payload: dict[str, Any] = {
        "timestamp_series": ordered_timestamps,
        "close_series": close_series,
        "ema20_series": ema20_series,
        "macd_series": macd_hist_series,
        "macd_dif_series": macd_dif_series,
        "macd_signal_series": macd_signal_series,
        "rsi7_series": rsi7_series,
        "ema20": format_value(_last_non_none(ema20_series)),
        "macd": format_value(_last_non_none(macd_hist_series)),
        "macd_signal": format_value(_last_non_none(macd_signal_series)),
        "macd_dif": format_value(_last_non_none(macd_dif_series)),
        "rsi7": format_value(_last_non_none(rsi7_series)),
    }
    return payload


def _extract_ordered_series(raw_candles: list[Any]) -> tuple[list[str], list[float]]:
    """Return timestamp+close series sorted from oldest to newest."""

    timestamps: list[str] = []
    closes: list[float] = []
    for candle in reversed(raw_candles):
        if not isinstance(candle, (list, tuple)) or len(candle) < 5:
            continue
        close_value = _safe_float(candle[4])
        if close_value is None:
            continue
        ts_value = str(candle[0])
        timestamps.append(ts_value)
        closes.append(close_value)
    return timestamps, closes


def calculate_ema_series(values: list[float], period: int) -> list[float | None]:
    """Calculate EMA over the provided closing prices."""

    if not values:
        return []

    ema_values: list[float | None] = []
    ema: float | None = None
    alpha = 2 / (period + 1)
    for idx, price in enumerate(values):
        if ema is None:
            ema = price
        else:
            ema = (price - ema) * alpha + ema
        if idx + 1 < period:
            ema_values.append(None)
        else:
            ema_values.append(ema)
    return ema_values


def calculate_macd_series(
    values: list[float],
) -> tuple[list[float | None], list[float | None], list[float | None]]:
    """Return DIF/DEA/MACD histogram series."""

    if not values:
        return [], [], []

    ema12: float | None = None
    ema26: float | None = None
    dea: float | None = None
    dif_series: list[float | None] = []
    dea_series: list[float | None] = []
    macd_series: list[float | None] = []

    alpha12 = 2 / (12 + 1)
    alpha26 = 2 / (26 + 1)
    alpha9 = 2 / (9 + 1)

    for idx, price in enumerate(values):
        ema12 = price if ema12 is None else (price - ema12) * alpha12 + ema12
        ema26 = price if ema26 is None else (price - ema26) * alpha26 + ema26

        if idx + 1 < 26:
            dif_series.append(None)
            dea_series.append(None)
            macd_series.append(None)
            continue

        dif = ema12 - ema26 if ema12 is not None and ema26 is not None else None
        dif_series.append(dif)
        if dif is None:
            dea_series.append(None)
            macd_series.append(None)
            continue

        dea = dif if dea is None else (dif - dea) * alpha9 + dea
        dea_series.append(dea)
        macd_series.append(2 * (dif - dea))

    return dif_series, dea_series, macd_series


def calculate_rsi_series(values: list[float], period: int) -> list[float | None]:
    """Calculate RSI with Wilder's smoothing."""

    if not values:
        return []

    rsi_values: list[float | None] = [None] * len(values)
    avg_gain: float | None = None
    avg_loss: float | None = None

    for idx in range(1, len(values)):
        delta = values[idx] - values[idx - 1]
        gain = max(delta, 0.0)
        loss = max(-delta, 0.0)

        if idx < period:
            avg_gain = (avg_gain or 0.0) + gain
            avg_loss = (avg_loss or 0.0) + loss
            continue

        if idx == period:
            avg_gain = ((avg_gain or 0.0) + gain) / period
            avg_loss = ((avg_loss or 0.0) + loss) / period
        else:
            avg_gain = ((avg_gain or 0.0) * (period - 1) + gain) / period
            avg_loss = ((avg_loss or 0.0) * (period - 1) + loss) / period

        if avg_loss == 0:
            rsi = 100.0
        elif avg_gain == 0:
            rsi = 0.0
        else:
            rs = (avg_gain or 0.0) / (avg_loss or 1e-12)
            rsi = 100 - (100 / (1 + rs))

        rsi_values[idx] = rsi

    return rsi_values


def _last_non_none(values: list[float | None]) -> float | None:
    """Return the last non-None entry in a series."""

    for value in reversed(values):
        if value is not None:
            return value
    return None


def _safe_float(value: Any) -> float | None:
    """Convert a value to float if possible."""

    try:
        return float(value)
    except (TypeError, ValueError):
        return None


logger = get_logger(__name__)
okx_client = OKXClient()

RETRYABLE_EXCEPTIONS: tuple[type[Exception], ...] = (
    httpx.ConnectError,
    httpx.ReadTimeout,
    httpx.WriteTimeout,
    httpx.RemoteProtocolError,
    httpx.TimeoutException,
    httpx.TransportError,
)
