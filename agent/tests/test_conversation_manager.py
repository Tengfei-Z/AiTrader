import pytest

from agent.llm.schemas.chat import ChatMessage
from agent.llm.services.conversation_manager import ConversationManager


@pytest.mark.asyncio
async def test_conversation_manager_stores_messages():
    manager = ConversationManager(max_history=2)
    session_id = "test-session"

    await manager.add_message(session_id, ChatMessage(role="user", content="hello"))
    await manager.add_message(session_id, ChatMessage(role="assistant", content="hi there"))
    await manager.add_message(session_id, ChatMessage(role="user", content="how are you?"))

    history = await manager.get_history(session_id, limit=5)
    assert len(history) == 2
    assert history[0].content == "hi there"
    assert history[1].content == "how are you?"

    await manager.clear_session(session_id)
    assert await manager.get_history(session_id, limit=5) == []
