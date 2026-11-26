from decimal import Decimal

import pytest

from agent.mcp.tools import trade


def test_normalize_generic_attached_algo_fields_for_short_order() -> None:
    payload = {
        "instId": "BTC-USDT-SWAP",
        "side": "sell",
        "ordType": "limit",
        "sz": "402",
        "px": "71050",
        "attachAlgoOrds": [
            {
                "algoClOrdId": "stop_loss_btc_001",
                "algoOrdType": "conditional",
                "algoSide": "buy",
                "triggerPx": "71250",
                "px": "71250",
                "sz": "402",
                "tpTriggerPxType": "last",
                "callbackRatio": "",
            },
            {
                "algoClOrdId": "take_profit_btc_001",
                "algoOrdType": "conditional",
                "algoSide": "buy",
                "triggerPx": "70500",
                "px": "70500",
                "sz": "402",
            },
        ],
    }

    normalized = trade._normalize_attach_algo_orders(payload)

    assert normalized == [
        {
            "attachAlgoClOrdId": "stop_loss_btc_001",
            "sz": "402",
            "slTriggerPx": "71250",
            "slOrdPx": "71250",
            "slTriggerPxType": "last",
        },
        {
            "attachAlgoClOrdId": "take_profit_btc_001",
            "sz": "402",
            "tpTriggerPx": "70500",
            "tpOrdPx": "70500",
            "tpTriggerPxType": "last",
        },
    ]


def test_normalize_attached_algo_fields_by_price_for_long_order() -> None:
    payload = {
        "instId": "BTC-USDT-SWAP",
        "side": "buy",
        "ordType": "limit",
        "sz": "400",
        "px": "71350",
        "attachAlgoOrds": [
            {"triggerPx": "71150", "px": "71150", "sz": "400"},
            {"triggerPx": "71500", "px": "71500", "sz": "400"},
        ],
    }

    normalized = trade._normalize_attach_algo_orders(payload)

    assert normalized == [
        {
            "sz": "400",
            "slTriggerPx": "71150",
            "slOrdPx": "71150",
            "slTriggerPxType": "last",
        },
        {
            "sz": "400",
            "tpTriggerPx": "71500",
            "tpOrdPx": "71500",
            "tpTriggerPxType": "last",
        },
    ]


@pytest.mark.asyncio
async def test_cap_open_order_size_keeps_margin_buffer(monkeypatch: pytest.MonkeyPatch) -> None:
    async def fake_fetch_instrument_spec(_: str) -> dict[str, str]:
        return {"ctVal": "0.01", "ctMult": "1", "lotSz": "0.01", "minSz": "0.01"}

    async def fake_fetch_available_usdt() -> Decimal:
        return Decimal("57164.55305612237")

    monkeypatch.setattr(trade, "_fetch_instrument_spec", fake_fetch_instrument_spec)
    monkeypatch.setattr(trade, "_fetch_available_usdt", fake_fetch_available_usdt)

    payload = {
        "instId": "BTC-USDT-SWAP",
        "side": "buy",
        "ordType": "limit",
        "sz": "400",
        "px": "71350",
        "lever": "5",
        "attachAlgoOrds": [{"sz": "400", "tpTriggerPx": "71500", "tpOrdPx": "71500"}],
    }

    await trade._cap_open_order_size(payload)

    assert payload["sz"] == "380.56"
    assert payload["attachAlgoOrds"][0]["sz"] == "380.56"


@pytest.mark.asyncio
async def test_prepare_order_payload_converts_buy_opponent_to_ask_price(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    async def fake_fetch_opponent_price(inst_id: str, side: str) -> Decimal:
        assert inst_id == "BTC-USDT-SWAP"
        assert side == "buy"
        return Decimal("70650.12")

    async def fake_cap_open_order_size(payload: dict[str, object]) -> None:
        assert payload["ordType"] == "limit"
        assert payload["px"] == "70650.12"

    monkeypatch.setattr(trade, "_fetch_opponent_price", fake_fetch_opponent_price)
    monkeypatch.setattr(trade, "_cap_open_order_size", fake_cap_open_order_size)

    payload = await trade._prepare_order_payload(
        {
            "instId": "BTC-USDT-SWAP",
            "side": "buy",
            "ordType": "opponent",
            "sz": "1",
            "lever": "5",
        }
    )

    assert payload["ordType"] == "limit"
    assert payload["px"] == "70650.12"
    assert payload["tdMode"] == "cross"
    assert payload["posSide"] == "net"


@pytest.mark.asyncio
async def test_fetch_opponent_price_uses_bid_for_sell(monkeypatch: pytest.MonkeyPatch) -> None:
    class FakeOkxClient:
        async def get_order_book(self, inst_id: str, depth: int) -> dict[str, object]:
            assert inst_id == "BTC-USDT-SWAP"
            assert depth == 1
            return {
                "code": "0",
                "data": [
                    {
                        "asks": [["70651.2", "12", "0", "1"]],
                        "bids": [["70650.8", "8", "0", "1"]],
                    }
                ],
            }

    monkeypatch.setattr(trade, "okx_client", FakeOkxClient())

    price = await trade._fetch_opponent_price("BTC-USDT-SWAP", "sell")

    assert price == Decimal("70650.8")


@pytest.mark.asyncio
async def test_opponent_order_retries_after_unfilled_limit(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    prices = iter([Decimal("70650.8"), Decimal("70649.2")])

    async def fake_fetch_opponent_price(_: str, __: str) -> Decimal:
        return next(prices)

    statuses = {
        "ord-1": {"ordId": "ord-1", "state": "live", "accFillSz": "0"},
        "ord-2": {"ordId": "ord-2", "state": "filled", "accFillSz": "1"},
    }

    class FakeOkxClient:
        def __init__(self) -> None:
            self.placed: list[dict[str, object]] = []
            self.canceled: list[str] = []

        async def place_order(self, payload: dict[str, object]) -> dict[str, object]:
            self.placed.append(payload)
            ord_id = f"ord-{len(self.placed)}"
            return {"code": "0", "data": [{"ordId": ord_id, "sCode": "0"}]}

        async def cancel_order(self, *, inst_id: str, order_id: str) -> dict[str, object]:
            assert inst_id == "BTC-USDT-SWAP"
            self.canceled.append(order_id)
            return {"code": "0", "data": [{"ordId": order_id, "sCode": "0"}]}

        async def get_order_details(self, *, inst_id: str, order_id: str) -> dict[str, object]:
            assert inst_id == "BTC-USDT-SWAP"
            return {"code": "0", "data": [{"ordId": order_id, "state": "live", "accFillSz": "0"}]}

    fake_client = FakeOkxClient()

    async def fake_confirm_order_status(*, inst_id: str, ord_id: str) -> dict[str, str]:
        assert inst_id == "BTC-USDT-SWAP"
        return statuses[ord_id]

    monkeypatch.setattr(trade, "okx_client", fake_client)
    monkeypatch.setattr(trade, "_fetch_opponent_price", fake_fetch_opponent_price)
    monkeypatch.setattr(trade, "_confirm_order_status", fake_confirm_order_status)

    response = await trade._place_opponent_order_with_confirmation(
        {
            "instId": "BTC-USDT-SWAP",
            "side": "sell",
            "ordType": "limit",
            "sz": "1",
            "px": "70650.8",
            "_aitraderOriginalOrdType": "opponent",
        }
    )

    assert response["aitraderConfirmation"]["status"] == "filled"
    assert fake_client.canceled == ["ord-1"]
    assert fake_client.placed == [
        {
            "instId": "BTC-USDT-SWAP",
            "side": "sell",
            "ordType": "limit",
            "sz": "1",
            "px": "70650.8",
        },
        {
            "instId": "BTC-USDT-SWAP",
            "side": "sell",
            "ordType": "limit",
            "sz": "1",
            "px": "70649.2",
        },
    ]


@pytest.mark.asyncio
async def test_opponent_order_falls_back_to_market(monkeypatch: pytest.MonkeyPatch) -> None:
    async def fake_fetch_opponent_price(_: str, __: str) -> Decimal:
        return Decimal("70650.8")

    class FakeOkxClient:
        def __init__(self) -> None:
            self.placed: list[dict[str, object]] = []

        async def place_order(self, payload: dict[str, object]) -> dict[str, object]:
            self.placed.append(payload)
            ord_id = f"ord-{len(self.placed)}"
            return {"code": "0", "data": [{"ordId": ord_id, "sCode": "0"}]}

        async def cancel_order(self, *, inst_id: str, order_id: str) -> dict[str, object]:
            return {"code": "0", "data": [{"ordId": order_id, "sCode": "0"}]}

        async def get_order_details(self, *, inst_id: str, order_id: str) -> dict[str, object]:
            assert inst_id == "BTC-USDT-SWAP"
            return {"code": "0", "data": [{"ordId": order_id, "state": "live", "accFillSz": "0"}]}

    fake_client = FakeOkxClient()

    async def fake_confirm_order_status(*, inst_id: str, ord_id: str) -> dict[str, str]:
        assert inst_id == "BTC-USDT-SWAP"
        if ord_id in {"ord-1", "ord-2", "ord-3"}:
            return {"ordId": ord_id, "state": "live", "accFillSz": "0"}
        return {"ordId": ord_id, "state": "filled", "accFillSz": "1"}

    monkeypatch.setattr(trade, "okx_client", fake_client)
    monkeypatch.setattr(trade, "_fetch_opponent_price", fake_fetch_opponent_price)
    monkeypatch.setattr(trade, "_confirm_order_status", fake_confirm_order_status)

    response = await trade._place_opponent_order_with_confirmation(
        {
            "instId": "BTC-USDT-SWAP",
            "side": "sell",
            "ordType": "limit",
            "sz": "1",
            "px": "70650.8",
            "_aitraderOriginalOrdType": "opponent",
        }
    )

    assert response["aitraderConfirmation"]["status"] == "filled"
    assert fake_client.placed[-1] == {
        "instId": "BTC-USDT-SWAP",
        "side": "sell",
        "ordType": "market",
        "sz": "1",
    }


@pytest.mark.asyncio
async def test_opponent_order_refreshes_fill_after_cancel(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    prices = iter([Decimal("70650.8"), Decimal("70649.2")])

    async def fake_fetch_opponent_price(_: str, __: str) -> Decimal:
        return next(prices)

    call_counts: dict[str, int] = {}

    class FakeOkxClient:
        def __init__(self) -> None:
            self.placed: list[dict[str, object]] = []

        async def place_order(self, payload: dict[str, object]) -> dict[str, object]:
            self.placed.append(payload)
            ord_id = f"ord-{len(self.placed)}"
            return {"code": "0", "data": [{"ordId": ord_id, "sCode": "0"}]}

        async def cancel_order(self, *, inst_id: str, order_id: str) -> dict[str, object]:
            assert inst_id == "BTC-USDT-SWAP"
            return {"code": "0", "data": [{"ordId": order_id, "sCode": "0"}]}

    fake_client = FakeOkxClient()

    async def fake_confirm_order_status(*, inst_id: str, ord_id: str) -> dict[str, str]:
        assert inst_id == "BTC-USDT-SWAP"
        call_counts[ord_id] = call_counts.get(ord_id, 0) + 1
        if ord_id == "ord-1" and call_counts[ord_id] == 1:
            return {"ordId": ord_id, "state": "live", "accFillSz": "0"}
        if ord_id == "ord-1":
            return {"ordId": ord_id, "state": "canceled", "accFillSz": "0.4"}
        return {"ordId": ord_id, "state": "filled", "accFillSz": "0.6"}

    monkeypatch.setattr(trade, "okx_client", fake_client)
    monkeypatch.setattr(trade, "_fetch_opponent_price", fake_fetch_opponent_price)
    monkeypatch.setattr(trade, "_confirm_order_status", fake_confirm_order_status)

    response = await trade._place_opponent_order_with_confirmation(
        {
            "instId": "BTC-USDT-SWAP",
            "side": "sell",
            "ordType": "limit",
            "sz": "1",
            "px": "70650.8",
            "_aitraderOriginalOrdType": "opponent",
        }
    )

    assert response["aitraderConfirmation"]["status"] == "filled"
    assert fake_client.placed[1]["sz"] == "0.6"


@pytest.mark.asyncio
async def test_cancel_open_orders_cancels_each_pending_order(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    class FakeOkxClient:
        def __init__(self) -> None:
            self.open_order_calls: list[dict[str, str | None]] = []
            self.canceled: list[dict[str, str]] = []

        async def get_open_orders(
            self,
            inst_type: str | None = None,
            inst_id: str | None = None,
            order_id: str | None = None,
        ) -> dict[str, object]:
            self.open_order_calls.append(
                {
                    "inst_type": inst_type,
                    "inst_id": inst_id,
                    "order_id": order_id,
                }
            )
            return {
                "code": "0",
                "data": [
                    {"ordId": "ord-1", "instId": inst_id},
                    {"ordId": "ord-2", "instId": inst_id},
                    {"instId": inst_id},
                ],
            }

        async def cancel_order(self, *, inst_id: str, order_id: str) -> dict[str, object]:
            self.canceled.append({"inst_id": inst_id, "order_id": order_id})
            return {"code": "0", "data": [{"ordId": order_id, "sCode": "0"}]}

    fake_client = FakeOkxClient()
    monkeypatch.setattr(trade, "okx_client", fake_client)

    response = await trade.cancel_open_orders_tool.fn(
        trade.CancelOpenOrdersInput(instId="BTC-USDT-SWAP")
    )

    assert fake_client.open_order_calls == [
        {
            "inst_type": "SWAP",
            "inst_id": "BTC-USDT-SWAP",
            "order_id": None,
        }
    ]
    assert fake_client.canceled == [
        {"inst_id": "BTC-USDT-SWAP", "order_id": "ord-1"},
        {"inst_id": "BTC-USDT-SWAP", "order_id": "ord-2"},
    ]
    assert response["ok"] is True
    assert response["code"] == "0"
    assert response["summary"] == {
        "instId": "BTC-USDT-SWAP",
        "checked": 3,
        "canceled": 2,
        "failed": 0,
    }


def test_position_tpsl_payload_uses_close_fraction_oco() -> None:
    payload = trade._position_tpsl_payload(
        trade.SetPositionTpslInput(
            instId="BTC-USDT-SWAP",
            side="sell",
            tpTriggerPx="71500",
            slTriggerPx="70500",
        )
    )

    assert payload == {
        "instId": "BTC-USDT-SWAP",
        "tdMode": "cross",
        "posSide": "net",
        "side": "sell",
        "ordType": "oco",
        "reduceOnly": "true",
        "closeFraction": "1",
        "tpTriggerPx": "71500",
        "tpOrdPx": "-1",
        "tpTriggerPxType": "last",
        "slTriggerPx": "70500",
        "slOrdPx": "-1",
        "slTriggerPxType": "last",
    }


@pytest.mark.asyncio
async def test_set_position_tpsl_places_algo_order(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    class FakeOkxClient:
        def __init__(self) -> None:
            self.placed_algo: list[dict[str, object]] = []

        async def place_algo_order(self, payload: dict[str, object]) -> dict[str, object]:
            self.placed_algo.append(payload)
            return {"code": "0", "data": [{"algoId": "algo-1", "sCode": "0"}]}

    fake_client = FakeOkxClient()
    monkeypatch.setattr(trade, "okx_client", fake_client)

    response = await trade.set_position_tpsl_tool.fn(
        trade.SetPositionTpslInput(
            instId="BTC-USDT-SWAP",
            side="sell",
            tpTriggerPx="71500",
            slTriggerPx="70500",
        )
    )

    assert response == {"code": "0", "data": [{"algoId": "algo-1", "sCode": "0"}]}
    assert fake_client.placed_algo == [
        {
            "instId": "BTC-USDT-SWAP",
            "tdMode": "cross",
            "posSide": "net",
            "side": "sell",
            "ordType": "oco",
            "reduceOnly": "true",
            "closeFraction": "1",
            "tpTriggerPx": "71500",
            "tpOrdPx": "-1",
            "tpTriggerPxType": "last",
            "slTriggerPx": "70500",
            "slOrdPx": "-1",
            "slTriggerPxType": "last",
        }
    ]


def test_opponent_confirmation_marks_final_and_canceled_orders() -> None:
    response = trade._with_opponent_confirmation(
        {"code": "0", "data": [{"ordId": "ord-2"}]},
        [
            {
                "attempt": 1,
                "action": "place_opponent_limit",
                "ordId": "ord-1",
                "state": "live",
                "filledSz": "0",
                "cancelResponse": {"code": "0"},
            },
            {
                "attempt": 2,
                "action": "place_opponent_limit",
                "ordId": "ord-2",
                "state": "filled",
                "filledSz": "1",
            },
        ],
        "filled",
    )

    confirmation = response["aitraderConfirmation"]
    assert confirmation["finalOrdId"] == "ord-2"
    assert confirmation["canceledOrdIds"] == ["ord-1"]
    assert confirmation["totalFilledSz"] == "1"
