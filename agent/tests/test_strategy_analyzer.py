import pytest

from agent.llm.schemas.analysis import AnalysisRequest
from agent.llm.services.strategy_analyzer import strategy_analyzer


@pytest.mark.asyncio
async def test_strategy_analyzer_uses_defaults(monkeypatch):
    async def fake_chat_completion(messages, **kwargs):
        user_msg = messages[-1].content
        assert "Instrument: BTC-USDT-SWAP" in user_msg
        assert "Analysis type: market_overview" in user_msg
        return {"choices": [{"message": {"content": "Summary\n- Item 1"}}], "usage": {}}

    monkeypatch.setattr(
        "agent.llm.services.strategy_analyzer.deepseek_client.chat_completion",
        fake_chat_completion,
    )
    monkeypatch.setattr("agent.llm.services.strategy_analyzer.get_tools_schema", lambda: [])

    response = await strategy_analyzer.analyze(AnalysisRequest(session_id="s"))

    assert response.summary.startswith("Summary")
    assert response.suggestions == ["Item 1"]
