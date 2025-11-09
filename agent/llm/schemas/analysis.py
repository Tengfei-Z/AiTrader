"""Pydantic schemas for analysis endpoints."""

from datetime import datetime
from typing import Literal

from pydantic import BaseModel, Field


class AnalysisRequest(BaseModel):
    session_id: str = Field(..., description="Conversation session identifier")


class AnalysisResponse(BaseModel):
    session_id: str
    instrument_id: str
    analysis_type: str
    summary: str
    suggestions: list[str] = Field(default_factory=list)
    created_at: datetime
