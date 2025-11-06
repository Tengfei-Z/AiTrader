"""Domain-specific analysis helpers."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import Any

from ...core.okx_client import okx_client

from ...core.logging_config import get_logger
from ..schemas.analysis import AnalysisRequest, AnalysisResponse
from ..schemas.chat import ChatMessage
from .conversation_manager import conversation_manager
from .deepseek_client import deepseek_client

logger = get_logger(__name__)


class StrategyAnalyzer:
    async def _market_snapshot(self, instrument_id: str) -> str:
        """Gather key OKX metrics for prompting."""

        try:
            ticker = await okx_client.get_ticker(instrument_id)
            raw_ticker = (ticker.get("data") or [{}])[0] if isinstance(ticker, dict) else {}
        except Exception as exc:  # pragma: no cover - network
            logger.warning("ticker_fetch_failed", instrument=instrument_id, error=str(exc))
            raw_ticker = {}

        try:
            candles = await okx_client.get_candles(instrument_id, bar="1m", limit=5)
        except Exception as exc:  # pragma: no cover - network
            logger.warning("candles_fetch_failed", instrument=instrument_id, error=str(exc))
            candles = []

        bid = raw_ticker.get("bidPx")
        ask = raw_ticker.get("askPx")
        last = raw_ticker.get("last")
        change_pct = raw_ticker.get("sodUtc0")

        parts = [
            f"Last price: {last}",
            f"Bid: {bid}",
            f"Ask: {ask}",
            f"Change since UTC open: {change_pct}",
            f"Recent candles: {candles}",
        ]
        return "\n".join(part for part in parts if part)

    async def analyze(self, request: AnalysisRequest) -> AnalysisResponse:
        system_prompt = (
            "You are an experienced quantitative trading analyst. Provide concise, actionable "
            "insights based on the latest OKX market data and user context."
        )

        history = await conversation_manager.get_history(request.session_id, limit=5)

        messages: list[ChatMessage] = [ChatMessage(role="system", content=system_prompt)]
        messages.extend(history)

        market_context = await self._market_snapshot(request.instrument_id)

        messages.append(
            ChatMessage(
                role="user",
                content=(
                    f"Instrument: {request.instrument_id}\n"
                    f"Analysis type: {request.analysis_type}\n"
                    f"Context: {request.context or 'None'}\n"
                    f"Market snapshot:\n{market_context}"
                ),
            )
        )

        result = await deepseek_client.chat_completion(messages, temperature=0.4)

        choice = result.get("choices", [{}])[0].get("message", {})
        summary = choice.get("content", "No analysis generated.")

        suggestions = []
        if isinstance(choice.get("content"), str):
            suggestions = [
                line.strip("- ").strip()
                for line in choice["content"].splitlines()
                if line.strip().startswith(("-", "1", "2", "3"))
            ]

        response = AnalysisResponse(
            session_id=request.session_id,
            instrument_id=request.instrument_id,
            analysis_type=request.analysis_type,
            summary=summary,
            suggestions=[s for s in suggestions if s],
            created_at=datetime.now(tz=timezone.utc),
        )

        await conversation_manager.add_message(
            request.session_id,
            ChatMessage(role="assistant", content=summary),
        )

        return response


strategy_analyzer = StrategyAnalyzer()
