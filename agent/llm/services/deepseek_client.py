"""DeepSeek API client abstraction."""

from __future__ import annotations

from typing import Any, Iterable

import httpx
from tenacity import AsyncRetrying, RetryError, retry_if_exception_type, stop_after_attempt, wait_exponential

from ...core.config import get_settings
from ...core.exceptions import ExternalServiceError, RateLimitExceeded
from ...core.http_client import async_http_client
from ...core.logging_config import get_logger
from ..schemas.chat import ChatMessage

logger = get_logger(__name__)


class DeepSeekClient:
    def __init__(self) -> None:
        self._settings = get_settings()

    async def _post(self, path: str, payload: dict[str, Any]) -> Any:
        headers = {
            "Authorization": f"Bearer {self._settings.deepseek_api_key.get_secret_value()}",
            "Content-Type": "application/json",
        }

        response: httpx.Response | None = None

        async for attempt in AsyncRetrying(
            wait=wait_exponential(multiplier=1, min=1, max=8),
            stop=stop_after_attempt(3),
            retry=retry_if_exception_type((httpx.RequestError, RateLimitExceeded)),
            reraise=True,
        ):
            with attempt:
                async with async_http_client(
                    base_url=str(self._settings.deepseek_api_base), timeout=30.0
                ) as client:
                    response = await client.post(path, json=payload, headers=headers)

                if response.status_code == httpx.codes.TOO_MANY_REQUESTS:
                    raise RateLimitExceeded(response.text)

                if response.is_error:
                    raise ExternalServiceError(
                        f"DeepSeek responded with {response.status_code}: {response.text}"
                    )

        if response is None:
            raise ExternalServiceError("DeepSeek request failed without response")

        logger.debug(
            "deepseek_request",
            path=path,
            status_code=response.status_code,
            remaining=response.headers.get("x-ratelimit-remaining"),
        )

        return response.json()

    async def chat_completion(
        self,
        messages: Iterable[ChatMessage],
        *,
        tools: list[dict[str, Any]] | None = None,
        tool_choice: dict[str, Any] | str | None = None,
        temperature: float = 0.7,
        response_format: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Issue a chat completion request to DeepSeek."""

        payload: dict[str, Any] = {
            "model": "deepseek-chat",
            "messages": [message.model_dump() for message in messages],
            "temperature": temperature,
        }
        if tools:
            payload["tools"] = tools
        if tool_choice:
            payload["tool_choice"] = tool_choice
        if response_format:
            payload["response_format"] = response_format

        logger.info(
            "deepseek_chat_request",
            tool_count=len(tools or []),
            tool_choice=tool_choice or "auto",
            temperature=temperature,
        )

        try:
            return await self._post("/chat/completions", payload)
        except RetryError as exc:
            raise ExternalServiceError("DeepSeek request failed after retries") from exc


deepseek_client = DeepSeekClient()
