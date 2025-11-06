"""FastAPI application entry point."""

from contextlib import asynccontextmanager

from fastapi import FastAPI

from ..core.config import get_settings
from ..core.logging_config import configure_logging, get_logger
from ..mcp.server import refresh_tools_schema
from .api.analysis import router as analysis_router
from .api.chat import router as chat_router
from .api.health import router as health_router

configure_logging()
logger = get_logger(__name__)


@asynccontextmanager
def lifespan(app: FastAPI):
    """FastAPI lifespan handler."""

    settings = get_settings()
    logger.info(
        "agent_startup",
        env=settings.app_env,
        log_level=settings.log_level,
    )
    refresh_tools_schema()
    yield
    logger.info("agent_shutdown")


app = FastAPI(
    title="AiTrader Agent",
    version="0.1.0",
    description="Python AI Agent service for AiTrader.",
    lifespan=lifespan,
)

app.include_router(health_router)
app.include_router(chat_router)
app.include_router(analysis_router)


@app.get("/")
async def index() -> dict[str, str]:
    return {"service": "aitrader-agent", "status": "ok"}
