"""Analysis endpoints."""

from fastapi import APIRouter, HTTPException

from ..schemas.analysis import AnalysisRequest, AnalysisResponse
from ..services.strategy_analyzer import strategy_analyzer

router = APIRouter(prefix="/analysis", tags=["analysis"])


@router.post("/", response_model=AnalysisResponse)
async def analyze_market(request: AnalysisRequest) -> AnalysisResponse:
    try:
        return await strategy_analyzer.analyze(request)
    except Exception as exc:  # pragma: no cover - FastAPI converts
        raise HTTPException(status_code=502, detail=str(exc)) from exc
