"""Chat endpoints with DeepSeek + FastMCP orchestration."""

import json
from datetime import datetime, timezone
from typing import Any

from fastapi import APIRouter, HTTPException

from ...core.logging_config import get_logger
from ...mcp.server import call_tool, get_tools_schema
from ..schemas.chat import ChatMessage, ChatRequest, ChatResponse, ChatUsage
from ..services.conversation_manager import conversation_manager
from ..services.deepseek_client import deepseek_client

router = APIRouter(prefix="/chat", tags=["chat"])
logger = get_logger(__name__)


def _extract_usage(blob: dict[str, Any] | None) -> ChatUsage:
    blob = blob or {}
    return ChatUsage(
        prompt_tokens=blob.get("prompt_tokens"),
        completion_tokens=blob.get("completion_tokens"),
        total_tokens=blob.get("total_tokens"),
    )


async def _handle_tool_calls(
    messages: list[ChatMessage],
    tool_calls: list[dict[str, Any]],
) -> list[ChatMessage]:
    tool_messages: list[ChatMessage] = []
    for tool_call in tool_calls:
        name = tool_call.get("function", {}).get("name")
        arguments_raw = tool_call.get("function", {}).get("arguments", "{}")
        try:
            arguments = json.loads(arguments_raw) if isinstance(arguments_raw, str) else arguments_raw
        except json.JSONDecodeError as exc:
            raise HTTPException(status_code=400, detail=f"Invalid tool arguments: {exc}") from exc

        result = await call_tool(name, arguments or {})
        logger.info("tool_call_executed", tool=name, arguments=arguments, result_summary=str(result)[:200])
        tool_message = ChatMessage(
            role="tool",
            content=json.dumps(result, ensure_ascii=False),
            name=name,
            tool_call_id=tool_call.get("id"),
        )
        messages.append(tool_message)
        tool_messages.append(tool_message)
    return tool_messages


@router.post("/", response_model=ChatResponse)
async def create_chat_completion(request: ChatRequest) -> ChatResponse:
    logger.info(
        "chat_request_received",
        session_id=request.session_id,
        use_history=request.use_history,
        history_limit=request.history_limit,
    )
    history: list[ChatMessage] = []
    if request.use_history:
        history = await conversation_manager.get_history(request.session_id, request.history_limit)

    messages: list[ChatMessage] = []
    if request.system_prompt:
        messages.append(ChatMessage(role="system", content=request.system_prompt))

    messages.extend(history)

    user_message = ChatMessage(role="user", content=request.message)
    messages.append(user_message)

    tools_schema = get_tools_schema()

    try:
        result = await deepseek_client.chat_completion(
            messages,
            tools=tools_schema,
            tool_choice={"type": "auto"},
        )
    except Exception as exc:  # pragma: no cover
        logger.exception("chat_completion_failed", session_id=request.session_id)
        raise HTTPException(status_code=502, detail=str(exc)) from exc

    choice = (result.get("choices") or [{}])[0].get("message") or {}
    tool_calls = choice.get("tool_calls") or []

    if tool_calls:
        await _handle_tool_calls(messages, tool_calls)
        try:
            result = await deepseek_client.chat_completion(messages)
        except Exception as exc:  # pragma: no cover
            logger.exception("chat_completion_followup_failed", session_id=request.session_id)
            raise HTTPException(status_code=502, detail=str(exc)) from exc
        choice = (result.get("choices") or [{}])[0].get("message") or {}

    reply = choice.get("content", "No response generated.")

    usage = _extract_usage(result.get("usage"))

    await conversation_manager.add_message(request.session_id, user_message)
    await conversation_manager.add_message(
        request.session_id, ChatMessage(role="assistant", content=reply)
    )

    reply_message = ChatResponse(
        session_id=request.session_id,
        reply=reply,
        usage=usage,
        created_at=datetime.now(tz=timezone.utc),
    )

    logger.info(
        "chat_request_completed",
        session_id=request.session_id,
        tool_calls=len(tool_calls),
        tokens_total=usage.total_tokens,
    )

    return reply_message
