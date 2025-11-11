"""Pydantic schemas for analysis endpoints."""

from datetime import datetime

from pydantic import BaseModel, Field


class AnalysisRequest(BaseModel):
    pass


class AnalysisResponse(BaseModel):
    summary: str
    suggestions: list[str] = Field(default_factory=list)
    created_at: datetime
