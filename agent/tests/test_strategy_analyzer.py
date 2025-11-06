import pytest

from agent.llm.schemas.analysis import AnalysisRequest
from agent.llm.services.strategy_analyzer import strategy_analyzer


@pytest.mark.asyncio
async def test_strategy_analyzer_includes_market_snapshot(monkeypatch):
    # Patch OKX client responses
    async def fake_get_ticker(inst_id):
        return {"data": [{"last": "100.0", "bidPx": "99.5", "askPx": "100.5", "sodUtc0": "95.0"}]}

    async def fake_get_candles(inst_id, bar, limit):
        return [["timestamp", "100", "101", "99", "100", "1000"]]

    monkeypatch.setattr("agent.llm.services.strategy_analyzer.okx_client.get_ticker", fake_get_ticker)
    monkeypatch.setattr("agent.llm.services.strategy_analyzer.okx_client.get_candles", fake_get_candles)

    async def fake_chat_completion(messages, **kwargs):
        # Ensure market snapshot made it into prompt
        prompt = messages[-1].content
        assert "Market snapshot" in prompt
        return {"choices": [{"message": {"content": "Summary\n- Item 1"}}], "usage": {}}

    monkeypatch.setattr(
        "agent.llm.services.strategy_analyzer.deepseek_client.chat_completion",
        fake_chat_completion,
    )

    response = await strategy_analyzer.analyze(
        AnalysisRequest(session_id="s", instrument_id="BTC-USDT", analysis_type="market_overview")
    )

    assert response.summary.startswith("Summary")
    assert response.suggestions == ["Item 1"]
