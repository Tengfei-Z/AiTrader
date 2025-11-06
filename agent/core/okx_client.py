"""Async OKX REST client."""

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


class OKXClient:
    """Minimal async client for OKX REST API."""

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

        headers: dict[str, str] = {
            "Content-Type": "application/json",
            "OK-ACCESS-PASSPHRASE": self._settings.okx_passphrase.get_secret_value(),
            "OK-ACCESS-TIMESTAMP": timestamp,
            "x-simulated-trading": "1",
        }

        if auth:
            sign_payload = f"{timestamp}{method.upper()}{path}{request_body}".encode("utf-8")
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

        async with async_http_client(
            base_url=self._settings.okx_base_url, timeout=15.0
        ) as client:
            response = await client.request(
                method=method.upper(),
                url=path,
                params=params,
                content=request_body if request_body else None,
                headers=headers,
            )

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

    async def get_ticker(self, inst_id: str) -> Any:
        """Fetch market ticker information for an instrument."""

        return await self._request(
            "GET",
            "/api/v5/market/ticker",
            params={"instId": inst_id},
            auth=False,
        )

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

    async def get_account_balance(self) -> Any:
        """Fetch account balance."""

        return await self._request("GET", "/api/v5/account/balance")

    async def get_positions(self, inst_type: str | None = None) -> Any:
        """Fetch current positions."""

        params = {"instType": inst_type} if inst_type else None
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


okx_client = OKXClient()
