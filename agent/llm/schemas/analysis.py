"""Pydantic schemas for analysis endpoints."""

from datetime import datetime

from pydantic import BaseModel


class AnalysisRequest(BaseModel):
    symbol: str | None = None
    trigger_context: str | None = None
    trigger_source: str | None = None


class AnalysisResponse(BaseModel):
    summary: str
    created_at: datetime
    symbol: str | None = None
