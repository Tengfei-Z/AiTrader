"""Pydantic schemas for chat endpoints."""

from __future__ import annotations

from datetime import datetime
from typing import Literal

from pydantic import BaseModel, Field


RoleLiteral = Literal["system", "user", "assistant", "tool"]


class ChatMessage(BaseModel):
    role: RoleLiteral
    content: str
    name: str | None = None
    tool_call_id: str | None = Field(None, description="Tool call identifier for tool messages")


class ChatRequest(BaseModel):
    session_id: str = Field(..., description="Conversation session identifier")
    message: str = Field(..., description="User message content")
    system_prompt: str | None = Field(None, description="Optional system-level directions")
    use_history: bool = Field(True, description="Whether to include conversation history")
    history_limit: int = Field(10, ge=0, le=50, description="Number of past messages to include")


class ChatUsage(BaseModel):
    prompt_tokens: int | None = None
    completion_tokens: int | None = None
    total_tokens: int | None = None


class ChatResponse(BaseModel):
    session_id: str
    reply: str
    created_at: datetime
    usage: ChatUsage | None = None
