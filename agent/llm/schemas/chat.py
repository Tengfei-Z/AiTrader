"""Pydantic schema for LLM chat messages."""

from __future__ import annotations

from typing import Any, Literal

from pydantic import BaseModel, Field


RoleLiteral = Literal["system", "user", "assistant", "tool"]


class ChatMessage(BaseModel):
    role: RoleLiteral
    content: str
    name: str | None = None
    tool_call_id: str | None = Field(None, description="Tool call identifier for tool messages")
    tool_calls: list[dict[str, Any]] | None = Field(
        default=None, description="Assistant-emitted tool calls"
    )
