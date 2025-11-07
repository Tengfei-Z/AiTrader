"""DeepSeek API client abstraction backed by the OpenAI SDK."""

from __future__ import annotations

from typing import Iterable

from openai import APIError as OpenAIError
from openai import AsyncOpenAI

from ...core.config import get_settings
from ...core.exceptions import ExternalServiceError
from ...core.logging_config import get_logger
from ..schemas.chat import ChatMessage

logger = get_logger(__name__)


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

    async def chat_completion(
        self,
        messages: Iterable[ChatMessage],
        *,
        tools: list[dict[str, Any]] | None = None,
        tool_choice: dict[str, Any] | str | None = None,
        temperature: float = 0.7,
        response_format: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Issue a chat completion request via the OpenAI SDK."""

        logger.info(
            "deepseek_chat_request",
            tool_count=len(tools or []),
            tool_choice=tool_choice or "auto",
            temperature=temperature,
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
                #response_format=response_format,
            )
        except OpenAIError as exc:  # pragma: no cover - network path
            logger.error(
                "deepseek_sdk_error",
                error_type=type(exc).__name__,
                message=str(exc),
            )
            raise ExternalServiceError(f"DeepSeek SDK error: {exc}") from exc

        return response.model_dump()


deepseek_client = DeepSeekClient()
