import json

import pytest
from httpx import AsyncClient

from agent.llm.main import app


@pytest.mark.asyncio
async def test_chat_endpoint_executes_tool_and_returns_reply(monkeypatch):
    calls = {"tool": [], "llm": []}

    async def fake_chat_completion(messages, **kwargs):
        calls["llm"].append({"messages": messages, "kwargs": kwargs})
        if kwargs.get("tools"):
            return {
                "choices": [
                    {
                        "message": {
                            "content": "",
                            "tool_calls": [
                                {
                                    "id": "tool-call-1",
                                    "function": {
                                        "name": "get_ticker",
                                        "arguments": json.dumps({"inst_id": "BTC-USDT"}),
                                    },
                                }
                            ],
                        }
                    }
                ],
                "usage": {},
            }
        return {
            "choices": [{"message": {"content": "Here is the result"}}],
            "usage": {"total_tokens": 10},
        }

    async def fake_call_tool(name, arguments):
        calls["tool"].append({"name": name, "arguments": arguments})
        return {"price": "100"}

    def fake_get_tools_schema():
        return [{"name": "get_ticker"}]

    monkeypatch.setattr("agent.llm.api.chat.deepseek_client.chat_completion", fake_chat_completion)
    monkeypatch.setattr("agent.llm.api.chat.call_tool", fake_call_tool)
    monkeypatch.setattr("agent.llm.api.chat.get_tools_schema", fake_get_tools_schema)

    async with AsyncClient(app=app, base_url="http://test") as client:
        response = await client.post(
            "/chat/",
            json={
                "session_id": "session",
                "message": "Get price",
            },
        )

    assert response.status_code == 200
    payload = response.json()
    assert payload["reply"] == "Here is the result"
    assert len(calls["tool"]) == 1
    assert calls["tool"][0]["name"] == "get_ticker"
    assert calls["tool"][0]["arguments"]["inst_id"] == "BTC-USDT"
