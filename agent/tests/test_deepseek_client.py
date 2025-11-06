import pytest
from pydantic import SecretStr

from agent.llm.schemas.chat import ChatMessage
from agent.llm.services.deepseek_client import deepseek_client


class DummySettings:
    deepseek_api_key = SecretStr("dummy")
    deepseek_api_base = "https://example.com"


@pytest.fixture(autouse=True)
def _patch_settings(monkeypatch):
    monkeypatch.setattr(deepseek_client, "_settings", DummySettings())


@pytest.mark.asyncio
async def test_chat_completion_builds_payload(monkeypatch):
    captured = {}

    async def fake_post(path, payload):
        captured["path"] = path
        captured["payload"] = payload
        return {"choices": []}

    monkeypatch.setattr(deepseek_client, "_post", fake_post)

    messages = [ChatMessage(role="user", content="hello")]

    await deepseek_client.chat_completion(
        messages,
        tools=[{"name": "tool", "description": "desc"}],
        tool_choice="auto",
        response_format={"type": "json_object"},
        temperature=0.1,
    )

    assert captured["path"] == "/chat/completions"
    assert captured["payload"]["tool_choice"] == "auto"
    assert captured["payload"]["response_format"]["type"] == "json_object"
    assert captured["payload"]["temperature"] == 0.1
