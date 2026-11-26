import pytest

from agent.mcp.tools import account


@pytest.mark.asyncio
async def test_get_open_orders_passes_filters_to_okx(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    class FakeOkxClient:
        def __init__(self) -> None:
            self.calls: list[dict[str, str | None]] = []

        async def get_open_orders(
            self,
            inst_type: str | None = None,
            inst_id: str | None = None,
            order_id: str | None = None,
        ) -> dict[str, object]:
            self.calls.append(
                {
                    "inst_type": inst_type,
                    "inst_id": inst_id,
                    "order_id": order_id,
                }
            )
            return {"code": "0", "data": [{"ordId": order_id, "instId": inst_id}]}

    fake_client = FakeOkxClient()
    monkeypatch.setattr(account, "okx_client", fake_client)

    response = await account.get_open_orders.fn(
        inst_type="SWAP",
        inst_id="BTC-USDT-SWAP",
        order_id="ord-1",
    )

    assert fake_client.calls == [
        {
            "inst_type": "SWAP",
            "inst_id": "BTC-USDT-SWAP",
            "order_id": "ord-1",
        }
    ]
    assert response == {"code": "0", "data": [{"ordId": "ord-1", "instId": "BTC-USDT-SWAP"}]}
