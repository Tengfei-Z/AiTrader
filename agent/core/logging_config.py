"""Structlog logging configuration."""

from __future__ import annotations

import logging
import sys
from pathlib import Path
from typing import Any

import structlog
from structlog.types import Processor

from .config import get_settings

_CONFIGURED = False


def _build_shared_processors() -> list[Processor]:
    timestamper = structlog.processors.TimeStamper(fmt="iso", utc=True)
    return [
        structlog.stdlib.add_log_level,
        timestamper,
        structlog.processors.StackInfoRenderer(),
        structlog.processors.format_exc_info,
    ]


def configure_logging() -> None:
    """Configure application-wide logging."""

    global _CONFIGURED
    if _CONFIGURED and logging.getLogger().handlers:
        return

    settings = get_settings()

    shared_processors = _build_shared_processors()

    structlog.configure(
        processors=[
            structlog.stdlib.filter_by_level,
            *shared_processors,
            structlog.stdlib.ProcessorFormatter.wrap_for_formatter,
        ],
        context_class=dict,
        logger_factory=structlog.stdlib.LoggerFactory(),
        wrapper_class=structlog.stdlib.BoundLogger,
        cache_logger_on_first_use=True,
    )

    console_formatter = structlog.stdlib.ProcessorFormatter(
        processors=[
            structlog.stdlib.ProcessorFormatter.remove_processors_meta,
            structlog.processors.dict_tracebacks,
            structlog.dev.ConsoleRenderer(colors=settings.app_env == "development"),
        ],
    )

    console_handler = logging.StreamHandler(sys.stdout)
    console_handler.setFormatter(console_formatter)
    console_handler.setLevel(settings.log_level)

    handlers: list[logging.Handler] = [console_handler]

    log_file = (settings.log_file or "").strip()
    if log_file:
        log_path = Path(log_file)
        log_path.parent.mkdir(parents=True, exist_ok=True)

        file_formatter = structlog.stdlib.ProcessorFormatter(
            processors=[
                structlog.stdlib.ProcessorFormatter.remove_processors_meta,
                structlog.processors.dict_tracebacks,
                structlog.processors.JSONRenderer(),
            ],
        )

        file_handler = logging.FileHandler(log_path, encoding="utf-8")
        file_handler.setFormatter(file_formatter)
        file_handler.setLevel(settings.log_level)
        handlers.append(file_handler)

    logging.basicConfig(
        handlers=handlers,
        level=settings.log_level,
        format="%(message)s",
    )

    _CONFIGURED = True


def get_logger(*args: Any, **kwargs: Any) -> structlog.stdlib.BoundLogger:
    """Return a configured structlog logger."""

    configure_logging()
    return structlog.get_logger(*args, **kwargs)
