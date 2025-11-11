"""Pydantic schemas for analysis endpoints."""

from datetime import datetime

from pydantic import BaseModel


class AnalysisRequest(BaseModel):
    pass


class AnalysisResponse(BaseModel):
    summary: str
    created_at: datetime
