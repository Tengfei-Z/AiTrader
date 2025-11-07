"""FastAPI application entry point."""

from contextlib import asynccontextmanager

from fastapi import FastAPI, Request

from ..core.config import env_file_candidates, get_settings, resolved_env_file
from ..core.logging_config import configure_logging, get_logger
from ..mcp.server import refresh_tools_schema
from .api.analysis import router as analysis_router
from .api.chat import router as chat_router
from .api.health import router as health_router

configure_logging()
logger = get_logger(__name__)


@asynccontextmanager
async def lifespan(app: FastAPI):
    """FastAPI lifespan handler."""

    settings = get_settings()
    logger.info(
        "agent_startup",
        env=settings.app_env,
        log_level=settings.log_level,
        agent_host=settings.agent_host,
        agent_port=settings.agent_port,
        deepseek_base=str(settings.deepseek_api_base),
        okx_base=str(settings.okx_base_url),
    )
    logger.info(
        "environment_loaded",
        log_file=settings.log_file or "stdout-only",
        env_file=resolved_env_file() or "not-found",
        env_candidates=list(env_file_candidates()),
    )
    await refresh_tools_schema()
    logger.info("mcp_tools_schema_ready")
    yield
    logger.info("agent_shutdown")


app = FastAPI(
    title="AiTrader Agent",
    version="0.1.0",
    description="Python AI Agent service for AiTrader.",
    lifespan=lifespan,
)


@app.middleware("http")
async def log_incoming_requests(request: Request, call_next):
    logger.info(
        "http_request_received",
        method=request.method,
        path=request.url.path,
        client=str(request.client[0]) if request.client else "unknown",
    )
    response = await call_next(request)
    logger.info(
        "http_request_completed",
        method=request.method,
        path=request.url.path,
        status_code=response.status_code,
    )
    return response

app.include_router(health_router)
app.include_router(chat_router)
app.include_router(analysis_router)


@app.get("/")
async def index() -> dict[str, str]:
    return {"service": "aitrader-agent", "status": "ok"}
