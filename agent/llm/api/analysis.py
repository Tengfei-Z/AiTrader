"""Analysis endpoints."""

from fastapi import APIRouter, HTTPException

from ...core.logging_config import get_logger
from ..schemas.analysis import AnalysisRequest, AnalysisResponse
from ..services.strategy_analyzer import strategy_analyzer

router = APIRouter(prefix="/analysis", tags=["analysis"])
logger = get_logger(__name__)


@router.post("/", response_model=AnalysisResponse)
async def analyze_market(request: AnalysisRequest) -> AnalysisResponse:
    logger.info("analysis_request_received")
    try:
        response = await strategy_analyzer.analyze(request)
    except Exception as exc:  # pragma: no cover - FastAPI converts
        logger.exception("analysis_request_failed")
        raise HTTPException(status_code=502, detail=str(exc)) from exc

    logger.info(
        "analysis_request_completed",
        suggestions=len(response.suggestions),
        summary_preview=response.summary[:120],
    )
    return response
