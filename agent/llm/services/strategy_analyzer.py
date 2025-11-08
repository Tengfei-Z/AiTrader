"""Domain-specific analysis helpers."""

from __future__ import annotations

import json
from datetime import datetime, timezone
from typing import Any

from ...core.logging_config import get_logger
from ...mcp.server import call_tool, get_tools_schema
from ..schemas.analysis import AnalysisRequest, AnalysisResponse
from ..schemas.chat import ChatMessage
from .conversation_manager import conversation_manager
from .deepseek_client import deepseek_client

logger = get_logger(__name__)

_SYSTEM_PROMPT = """你是一个专业的加密货币交易 AI，负责独立分析市场、制定交易计划并执行策略。目标是最大化风险调整后的收益（如 Sharpe Ratio），同时保障账户稳健运行。

工作职责：
1. 产出 Alpha：研判行情结构、识别交易机会、预测价格走向。
2. 决定仓位：合理分配资金、选择杠杆倍数、管理整体风险敞口。
3. 控制节奏：确定开仓与平仓时机，设置止盈止损。
4. 风险管理：避免过度暴露，确保有充足保证金与退出计划。

约束条件：
- 仅可交易白名单内的币种与合约。
- 杠杆上限 25X。
- 每个持仓必须具备完整的退出方案（止盈、止损、失效条件）。
- 输出需清晰、可审计，便于透明化展示。

可用 MCP 工具：
1. get_ticker：获取实时行情与盘口数据。
2. get_account_balance：查询账户状态与可用保证金。
3. get_positions：查看当前持仓与风险敞口。
4. place_order：执行交易（下单/平仓），需同步提供目标方向、价格、数量及杠杆。
5. cancel_order：撤销已提交的挂单。
6. get_order_history：回溯近期订单执行情况。

输出要求（每次响应）：
1. 思考总结（≤200 字）：概述市场状况、持仓状态、下一步计划。
2. 决策行动：如需操作，调用 MCP 工具并保证退出计划完整；若仅分析，也需说明原因。
3. 置信度（0-100）：给出当前判断的信心水平。

策略提示：
- 风险优先，追求稳定的风险收益比。
- 避免无效频繁交易，关注成本。
- 保持严格止损，保护本金。
- 同时评估多头与空头机会，方向以风险收益比与趋势信号为准。
- 当趋势明显向下，需果断执行空头策略并提供完整退出计划。
- 顺势而为，尊重趋势变化。
- 保持耐心，等待高质量信号。"""


class StrategyAnalyzer:
    async def analyze(self, request: AnalysisRequest) -> AnalysisResponse:
        instrument_id = "BTC-USDT-SWAP"
        analysis_type = "market_overview"

        history = await conversation_manager.get_history(request.session_id, limit=5)
        logger.info(
            "analysis_history_loaded",
            session_id=request.session_id,
            instrument=instrument_id,
            analysis_type=analysis_type,
            history_messages=len(history),
        )

        messages: list[ChatMessage] = [ChatMessage(role="system", content=_SYSTEM_PROMPT)]
        messages.extend(history)

        messages.append(
            ChatMessage(
                role="user",
                content=(
                    f"Instrument: {instrument_id}\n"
                    f"Analysis type: {analysis_type}\n"
                    "请严格按照职责行事，若需要行情、账户或交易操作，请自行调用 MCP 工具。"
                ),
            )
        )

        tools_schema = get_tools_schema()

        async def _dispatch_chat() -> dict[str, Any]:
            logger.info(
                "deepseek_analysis_dispatch",
                session_id=request.session_id,
                instrument=instrument_id,
                history=len(history),
            )
            return await deepseek_client.chat_completion(
                messages,
                temperature=0.4,
                tools=tools_schema,
                tool_choice="auto",
            )

        async def _handle_tool_calls(tool_calls: list[dict[str, Any]]) -> None:
            for tool_call in tool_calls:
                name = tool_call.get("function", {}).get("name")
                arguments_raw = tool_call.get("function", {}).get("arguments", "{}")
                logger.info(
                    "strategy_tool_call_received",
                    tool=name,
                    raw_arguments=arguments_raw,
                    tool_call_id=tool_call.get("id"),
                )
                try:
                    arguments = (
                        json.loads(arguments_raw) if isinstance(arguments_raw, str) else arguments_raw
                    ) or {}
                except json.JSONDecodeError as exc:
                    raise ValueError(f"Invalid tool arguments: {exc}") from exc

                result = await call_tool(name, arguments)
                logger.info(
                    "strategy_tool_executed",
                    tool=name,
                    arguments=arguments,
                    result_summary=str(result)[:800],
                )

                messages.append(
                    ChatMessage(
                        role="tool",
                        content=json.dumps(result, ensure_ascii=False),
                        name=name,
                        tool_call_id=tool_call.get("id"),
                    )
                )

        result = await _dispatch_chat()

        max_iterations = 3
        for iteration in range(1, max_iterations + 1):
            choice = result.get("choices", [{}])[0].get("message", {})
            assistant_message = ChatMessage(
                role="assistant",
                content=choice.get("content") or "",
                tool_calls=choice.get("tool_calls"),
            )
            messages.append(assistant_message)

            tool_calls = assistant_message.tool_calls or []
            if not tool_calls:
                logger.info(
                    "strategy_no_tool_calls",
                    session_id=request.session_id,
                    instrument=instrument_id,
                    iteration=iteration,
                )
                break

            logger.info(
                "strategy_tool_calls_detected",
                session_id=request.session_id,
                instrument=instrument_id,
                iteration=iteration,
                tool_count=len(tool_calls),
            )
            await _handle_tool_calls(tool_calls)

            if iteration == max_iterations:
                logger.warning(
                    "strategy_tool_loop_maxed",
                    session_id=request.session_id,
                    instrument=instrument_id,
                    max_iterations=max_iterations,
                )
                break

            result = await _dispatch_chat()

        choice = result.get("choices", [{}])[0].get("message", {})
        summary = choice.get("content", "No analysis generated.")
        logger.info(
            "strategy_final_response",
            session_id=request.session_id,
            instrument=instrument_id,
            summary_preview=summary[:800] if isinstance(summary, str) else str(summary)[:800],
        )

        suggestions = []
        if isinstance(choice.get("content"), str):
            suggestions = [
                line.strip("- ").strip()
                for line in choice["content"].splitlines()
                if line.strip().startswith(("-", "1", "2", "3"))
            ]

        response = AnalysisResponse(
            session_id=request.session_id,
            instrument_id=instrument_id,
            analysis_type=analysis_type,
            summary=summary,
            suggestions=[s for s in suggestions if s],
            created_at=datetime.now(tz=timezone.utc),
        )

        await conversation_manager.add_message(
            request.session_id,
            ChatMessage(role="assistant", content=summary),
        )

        logger.info(
            "analysis_response_prepared",
            session_id=request.session_id,
            suggestions=len(response.suggestions),
        )

        return response


strategy_analyzer = StrategyAnalyzer()
