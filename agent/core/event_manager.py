"""WebSocket manager for broadcasting agent events to subscribers."""

from __future__ import annotations

import asyncio
import json
from typing import Any, Set

from fastapi import WebSocket, WebSocketDisconnect

from .logging_config import get_logger

logger = get_logger(__name__)


class AgentEventManager:
    def __init__(self) -> None:
        self._connections: Set[WebSocket] = set()
        self._lock = asyncio.Lock()

    async def connect(self, websocket: WebSocket) -> None:
        await websocket.accept()
        async with self._lock:
            self._connections.add(websocket)
        logger.info("agent_event_connection_established")

    async def disconnect(self, websocket: WebSocket) -> None:
        async with self._lock:
            self._connections.discard(websocket)
        logger.info("agent_event_connection_closed")

    async def broadcast(self, payload: dict[str, Any]) -> None:
        async with self._lock:
            connections = list(self._connections)

        if not connections:
            return

        message = json.dumps(payload, ensure_ascii=False)

        for websocket in connections:
            try:
                await websocket.send_text(message)
            except WebSocketDisconnect:
                await self.disconnect(websocket)
            except Exception as exc:
                logger.warning(
                    "failed to push agent event to websocket",
                    error=str(exc),
                )
                await self.disconnect(websocket)


event_manager = AgentEventManager()


async def publish_task_result(payload: dict[str, Any]) -> None:
    """Broadcast the task result payload to all connected subscribers."""

    await event_manager.broadcast(payload)
