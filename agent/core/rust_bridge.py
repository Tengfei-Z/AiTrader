"""Helpers that notify the Rust backend about agent analysis results."""

from __future__ import annotations

import asyncio
import json
from typing import Any

import websockets

from .config import get_settings
from .logging_config import get_logger

logger = get_logger(__name__)


async def publish_task_result(payload: dict[str, Any]) -> None:
    """Send the structured task result payload to the Rust backend, if configured."""

    settings = get_settings()
    rust_ws_url = settings.rust_ws_url
    if rust_ws_url is None:
        logger.debug("Rust WebSocket URL not configured, skipping task result notify")
        return

    timeout = settings.rust_ws_timeout_seconds
    try:
        async with websockets.connect(str(rust_ws_url)) as websocket:
            await asyncio.wait_for(
                websocket.send(json.dumps(payload, ensure_ascii=False)),
                timeout=timeout,
            )

            try:
                ack = await asyncio.wait_for(websocket.recv(), timeout=timeout)
                logger.debug("Rust WebSocket acknowledged task result: %s", ack)
            except asyncio.TimeoutError:
                logger.debug("Rust WebSocket acknowledgment timed out")
    except (asyncio.TimeoutError, websockets.WebSocketException) as exc:
        logger.warning("failed to publish task result to Rust: %s", exc)
