"""In-memory conversation store."""

from __future__ import annotations

import asyncio
from collections import deque
from dataclasses import dataclass
from typing import Deque, Iterable, List

from ..schemas.chat import ChatMessage


@dataclass(slots=True)
class ConversationHistory:
    messages: Deque[ChatMessage]


class ConversationManager:
    """Manage short-lived conversation histories in memory."""

    def __init__(self, max_history: int = 50) -> None:
        self._max_history = max_history
        self._sessions: dict[str, ConversationHistory] = {}
        self._lock = asyncio.Lock()

    async def add_message(self, session_id: str, message: ChatMessage) -> None:
        async with self._lock:
            history = self._sessions.setdefault(
                session_id, ConversationHistory(messages=deque(maxlen=self._max_history))
            )
            history.messages.append(message)

    async def get_history(self, session_id: str, limit: int) -> List[ChatMessage]:
        async with self._lock:
            if session_id not in self._sessions:
                return []

            history = list(self._sessions[session_id].messages)
            if limit <= 0:
                return []
            return history[-limit:]

    async def iter_history(self, session_id: str) -> Iterable[ChatMessage]:
        async with self._lock:
            if session_id not in self._sessions:
                return []
            return list(self._sessions[session_id].messages)

    async def clear_session(self, session_id: str) -> None:
        async with self._lock:
            self._sessions.pop(session_id, None)


conversation_manager = ConversationManager(max_history=2)
