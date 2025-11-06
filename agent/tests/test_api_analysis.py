import pytest
from httpx import AsyncClient

from agent.llm.main import app


@pytest.mark.asyncio
async def test_analysis_endpoint_uses_market_snapshot(monkeypatch):
    async def fake_get_ticker(inst_id):
        return {"data": [{"last": "100", "bidPx": "99", "askPx": "101", "sodUtc0": "95"}]}

    async def fake_get_candles(inst_id, bar, limit):
        return [["ts", "100", "101", "99", "100", "500"]]

    async def fake_chat_completion(messages, **kwargs):
        assert "Market snapshot" in messages[-1].content
        return {"choices": [{"message": {"content": "Overview\n- Keep watching"}}], "usage": {}}

    monkeypatch.setattr("agent.llm.services.strategy_analyzer.okx_client.get_ticker", fake_get_ticker)
    monkeypatch.setattr("agent.llm.services.strategy_analyzer.okx_client.get_candles", fake_get_candles)
    monkeypatch.setattr(
        "agent.llm.services.strategy_analyzer.deepseek_client.chat_completion",
        fake_chat_completion,
    )

    async with AsyncClient(app=app, base_url="http://test") as client:
        response = await client.post(
            "/analysis/",
            json={
                "session_id": "session",
                "instrument_id": "BTC-USDT",
                "analysis_type": "market_overview",
            },
        )

    assert response.status_code == 200
    payload = response.json()
    assert payload["summary"].startswith("Overview")
    assert payload["suggestions"] == ["Keep watching"]
