import pytest

from agent.mcp.tools import market


@pytest.mark.asyncio
async def test_get_instrument_specs_accepts_direct_inst_id(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    class FakeOkxClient:
        def __init__(self) -> None:
            self.calls: list[dict[str, str | None]] = []

        async def get_instruments(
            self,
            *,
            inst_type: str,
            inst_id: str | None = None,
            underlying: str | None = None,
        ) -> dict[str, object]:
            self.calls.append(
                {
                    "inst_type": inst_type,
                    "inst_id": inst_id,
                    "underlying": underlying,
                }
            )
            return {"code": "0", "data": [{"instId": inst_id, "instType": inst_type}]}

    fake_client = FakeOkxClient()
    monkeypatch.setattr(market, "okx_client", fake_client)

    response = await market.get_instrument_specs.fn(instId="BTC-USDT-SWAP")

    assert fake_client.calls == [
        {
            "inst_type": "SWAP",
            "inst_id": "BTC-USDT-SWAP",
            "underlying": None,
        }
    ]
    assert response == {"code": "0", "data": [{"instId": "BTC-USDT-SWAP", "instType": "SWAP"}]}
