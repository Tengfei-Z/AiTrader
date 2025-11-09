import pytest
from httpx import AsyncClient

from agent.llm.main import app


@pytest.mark.asyncio
async def test_analysis_endpoint_invokes_deepseek_with_expected_prompt(monkeypatch):
    async def fake_chat_completion(messages, **kwargs):
        assert messages[0].role == "system"
        assert "加密货币交易 AI" in messages[0].content
        assert messages[-1].role == "user"
        assert "Instrument: BTC-USDT" in messages[-1].content
        assert kwargs["tool_choice"] == "auto"
        return {"choices": [{"message": {"content": "Overview\n- Keep watching"}}], "usage": {}}

    monkeypatch.setattr(
        "agent.llm.services.strategy_analyzer.deepseek_client.chat_completion",
        fake_chat_completion,
    )
    monkeypatch.setattr("agent.llm.services.strategy_analyzer.get_tools_schema", lambda: [])

    async with AsyncClient(app=app, base_url="http://test") as client:
        response = await client.post(
            "/analysis/",
            json={
                "session_id": "session",
            },
        )

    assert response.status_code == 200
    payload = response.json()
    assert payload["summary"].startswith("Overview")
    assert payload["suggestions"] == ["Keep watching"]
