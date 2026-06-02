"""DeepSeek API client abstraction backed by the OpenAI SDK."""

from __future__ import annotations

import asyncio
import json
from datetime import date
from pathlib import Path
from typing import Any, Iterable

from openai import APIError as OpenAIError
from openai import AsyncOpenAI

from ...core.config import get_settings
from ...core.exceptions import ExternalServiceError
from ...core.logging_config import get_logger
from ..schemas.chat import ChatMessage

logger = get_logger(__name__)


class DailyRequestBudget:
    """Persistent per-day request counter for model calls."""

    def __init__(self, limit: int, state_file: str) -> None:
        self._limit = limit
        self._state_file = Path(state_file)
        self._lock = asyncio.Lock()
        self._loaded = False
        self._day = date.today().isoformat()
        self._count = 0

    async def consume(self) -> tuple[int, int] | None:
        if self._limit <= 0:
            return None

        async with self._lock:
            if not self._loaded:
                self._load()

            today = date.today().isoformat()
            if self._day != today:
                self._day = today
                self._count = 0

            if self._count >= self._limit:
                logger.warning(
                    "deepseek_daily_budget_exhausted",
                    day=self._day,
                    used=self._count,
                    limit=self._limit,
                )
                raise ExternalServiceError(
                    f"DeepSeek daily request limit reached: {self._count}/{self._limit}"
                )

            self._count += 1
            self._save()
            return self._count, self._limit

    async def update_limit(self, limit: int) -> None:
        async with self._lock:
            self._limit = limit
            logger.info(
                "deepseek_daily_budget_limit_updated",
                limit=self._limit,
            )

    def _load(self) -> None:
        self._loaded = True
        try:
            payload = json.loads(self._state_file.read_text(encoding="utf-8"))
        except FileNotFoundError:
            return
        except (OSError, json.JSONDecodeError) as exc:
            logger.warning(
                "deepseek_budget_state_load_failed",
                path=str(self._state_file),
                error=str(exc),
            )
            return

        day = payload.get("day")
        count = payload.get("count")
        if isinstance(day, str) and isinstance(count, int) and count >= 0:
            self._day = day
            self._count = count

    def _save(self) -> None:
        try:
            self._state_file.parent.mkdir(parents=True, exist_ok=True)
            tmp_file = self._state_file.with_suffix(f"{self._state_file.suffix}.tmp")
            tmp_file.write_text(
                json.dumps({"day": self._day, "count": self._count}),
                encoding="utf-8",
            )
            tmp_file.replace(self._state_file)
        except OSError as exc:
            logger.warning(
                "deepseek_budget_state_save_failed",
                path=str(self._state_file),
                error=str(exc),
            )


class DeepSeekClient:
    """Thin wrapper around the OpenAI-compatible DeepSeek chat API."""

    def __init__(self) -> None:
        settings = get_settings()
        masked_key = f"{settings.deepseek_api_key.get_secret_value()[:4]}***{settings.deepseek_api_key.get_secret_value()[-4:]}"
        base_url_raw = str(settings.deepseek_api_base).rstrip("/")
        base_url = base_url_raw
        if not base_url.endswith("/v1"):
            base_url = f"{base_url}/v1"
        logger.info(
            "deepseek_client_init",
            base_url=base_url,
            api_key_masked=masked_key,
        )
        self._client = AsyncOpenAI(
            api_key=settings.deepseek_api_key.get_secret_value(),
            base_url=base_url,
        )
        self._budget = DailyRequestBudget(
            settings.deepseek_daily_request_limit,
            settings.deepseek_request_budget_file,
        )

    async def chat_completion(
        self,
        messages: Iterable[ChatMessage],
        *,
        tools: list[dict[str, Any]] | None = None,
        tool_choice: dict[str, Any] | str | None = None,
        temperature: float = 0.7,
        response_format: dict[str, Any] | None = None,
        count_against_budget: bool = True,
    ) -> dict[str, Any]:
        """Issue a chat completion request via the OpenAI SDK."""

        budget = await self._budget.consume() if count_against_budget else None
        logger.info(
            "deepseek_chat_request",
            tool_count=len(tools or []),
            tool_choice=tool_choice or "auto",
            temperature=temperature,
            count_against_budget=count_against_budget,
            daily_budget_used=budget[0] if budget else None,
            daily_budget_limit=budget[1] if budget else None,
        )

        payload_messages = [message.model_dump() for message in messages]
        logger.info(
            "deepseek_request_payload",
            message_count=len(payload_messages),
            preview=payload_messages[-1] if payload_messages else None,
        )

        try:
            response = await self._client.chat.completions.create(
                model="deepseek-chat",
                messages=payload_messages,
                temperature=temperature,
                tools=tools,
                tool_choice=tool_choice,
            )
        except OpenAIError as exc:  # pragma: no cover - network path
            logger.error(
                "deepseek_sdk_error",
                error_type=type(exc).__name__,
                message=str(exc),
            )
            raise ExternalServiceError(f"DeepSeek SDK error: {exc}") from exc

        return response.model_dump()

    async def update_daily_request_limit(self, limit: int) -> None:
        await self._budget.update_limit(limit)


deepseek_client = DeepSeekClient()
