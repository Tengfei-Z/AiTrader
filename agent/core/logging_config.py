"""Structlog logging configuration with plain-text output."""

from __future__ import annotations

import logging
from datetime import datetime, timezone
import sys
from pathlib import Path
from typing import Any

import structlog
from structlog.types import Processor

from .config import env_file_candidates, get_settings, resolved_env_file

_CONFIGURED = False


def _build_shared_processors() -> list[Processor]:
    timestamper = structlog.processors.TimeStamper(fmt="iso", utc=True)
    return [
        structlog.stdlib.add_log_level,
        timestamper,
        structlog.processors.StackInfoRenderer(),
        structlog.processors.format_exc_info,
    ]


def _plain_text_renderer(_: Any, event_name: str, event_dict: dict[str, Any]) -> str:
    """Render structlog events as human-friendly plain text."""

    timestamp = event_dict.pop("timestamp", datetime.now(tz=timezone.utc).isoformat())
    level = str(event_dict.pop("level", "info")).upper()
    event = event_dict.pop("event", "") or event_dict.pop("message", "") or event_name

    extras = " ".join(f"{key}={value}" for key, value in event_dict.items() if value is not None)
    parts = [timestamp, f"[{level}]", event]
    if extras:
        parts.append(extras)
    return " ".join(part for part in parts if part)


def configure_logging() -> None:
    """Configure application-wide logging."""

    global _CONFIGURED
    if _CONFIGURED and logging.getLogger().handlers:
        return

    settings = get_settings()
    env_file = resolved_env_file()

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
            _plain_text_renderer,
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
                _plain_text_renderer,
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

    # Silence noisy reload/watchdog logs when running with uvicorn --reload.
    for noisy in ("watchgod.watcher", "uvicorn.supervisors.watchgodreload"):
        logging.getLogger(noisy).setLevel(logging.WARNING)

    structlog.get_logger(__name__).info(
        "logging_configured",
        env=settings.app_env,
        level=settings.log_level,
        log_file=log_file or "stdout-only",
        env_file=env_file or "not-found",
        env_candidates=list(env_file_candidates()),
    )

    _CONFIGURED = True


def get_logger(*args: Any, **kwargs: Any) -> structlog.stdlib.BoundLogger:
    """Return a configured structlog logger."""

    configure_logging()
    return structlog.get_logger(*args, **kwargs)
