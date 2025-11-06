import json

import pytest
from fastapi import HTTPException

from agent.llm.api.chat import _handle_tool_calls
from agent.llm.schemas.chat import ChatMessage


@pytest.mark.asyncio
async def test_handle_tool_calls_executes_and_appends(monkeypatch):
    captured = {}

    async def fake_call_tool(name, arguments):
        captured["name"] = name
        captured["arguments"] = arguments
        return {"ok": True}

    monkeypatch.setattr("agent.llm.api.chat.call_tool", fake_call_tool)

    messages = []
    tool_calls = [
        {
            "id": "call-1",
            "function": {"name": "get_ticker", "arguments": json.dumps({"inst_id": "BTC-USDT"})},
        }
    ]

    tool_messages = await _handle_tool_calls(messages, tool_calls)

    assert captured == {"name": "get_ticker", "arguments": {"inst_id": "BTC-USDT"}}
    assert len(tool_messages) == 1
    assert tool_messages[0].role == "tool"
    assert json.loads(tool_messages[0].content)["ok"] is True
    assert messages[-1] is tool_messages[0]


@pytest.mark.asyncio
async def test_handle_tool_calls_invalid_json():
    messages = []
    tool_calls = [
        {"id": "call-1", "function": {"name": "broken", "arguments": "{not-json]"}},
    ]

    with pytest.raises(HTTPException):
        await _handle_tool_calls(messages, tool_calls)
