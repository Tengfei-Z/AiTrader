"""Domain-specific analysis helpers."""

from __future__ import annotations

import json
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

from ...core.event_manager import publish_task_result
from ...core.logging_config import get_logger
from ...mcp.server import call_tool, get_tools_schema
from ..schemas.analysis import AnalysisRequest, AnalysisResponse
from ..schemas.chat import ChatMessage
from .conversation_manager import conversation_manager
from .deepseek_client import deepseek_client

logger = get_logger(__name__)

@dataclass
class OrderEvent:
    ord_id: str

    def to_dict(self) -> dict[str, Any]:
        return {"ordId": self.ord_id}


class AgentEventCollector:
    def __init__(self) -> None:
        self.order_events: list[OrderEvent] = []

    def record_tool_call(
        self, name: str, arguments: dict[str, Any], result: dict[str, Any]
    ) -> None:
        if name != "place_order":
            return

        payloads = self._extract_payloads(result)
        logger.info(
            "strategy_place_order_result",
            tool=name,
            arguments=arguments,
            payload_count=len(payloads),
        )

        for entry in payloads:
            if not isinstance(entry, dict):
                continue
            ord_id = (
                entry.get("ordId")
                or entry.get("orderId")
                or entry.get("order_id")
                or entry.get("tradeId")
            )
            if not ord_id:
                logger.info(
                    "strategy_place_order_no_ord_id",
                    tool=name,
                    payload=entry,
                )
                continue
            self.order_events.append(OrderEvent(ord_id=ord_id))
            logger.info(
                "strategy_place_order_recorded",
                tool=name,
                ord_id=ord_id,
            )

    def _extract_payloads(self, result: dict[str, Any]) -> list[Any]:
        payload = result.get("data") or result.get("result")
        if payload is None:
            return []

        if isinstance(payload, dict):
            return [payload]
        if isinstance(payload, list):
            return payload
        return [payload]

_SYSTEM_PROMPT = """你是一个专业的加密货币交易 AI，负责独立分析市场、制定交易计划并执行策略。目标是最大化风险调整后的收益（如 Sharpe Ratio），同时保障账户稳健运行。

工作职责：
1. 产出 Alpha：研判行情结构、识别交易机会、预测价格走向。
2. 决定仓位：合理分配资金、选择杠杆倍数、管理整体风险敞口。
3. 控制节奏：确定开仓与平仓时机。
4. 风险管理：避免过度暴露，确保有充足保证金与退出计划。

- 仅可交易白名单内的币种与合约。
- 杠杆上限 25X。
- 输出需清晰、可审计，便于透明化展示。
- 分析前必须调用 `get_positions` 获取当前持仓，并确保账户至少保留一个方向性仓位（多头或空头）；若无仓位，需立即根据趋势择优开仓。

可用 MCP 工具：
1. get_ticker：获取实时行情与盘口数据。
2. get_instrument_specs：查询合约规格（lotSz/minSz/tickSz），**每次下单前必须调用以确认下单数量是最小交易单位的整倍数**。
3. get_account_balance：查询账户状态与可用保证金。
4. get_positions：查看当前持仓与风险敞口。
5. place_order：执行交易（下单/平仓），需同步提供目标方向、价格、数量及杠杆。
6. cancel_order：撤销已提交的挂单。

输出要求（每次响应）：
1. 思考总结（≤200 字）：概述市场状况、持仓状态、下一步计划。
2. 决策行动：如需操作，调用 MCP 工具并说明理由；若仅分析，也需说明原因。
3. 置信度（0-100）：给出当前判断的信心水平。

策略提示：
- 风险优先，追求稳定的风险收益比。
- 避免无效频繁交易，关注成本。
- 所有分析必须输出明确方向（多或空），并配合执行至少一个对应方向的订单。
- 多空两端都可考虑，方向由趋势与风险收益比决定。
- 顺势而为，尊重趋势变化。
- 保持耐心，等待高质量信号。

其它要求：
1. 下单默认使用 5x 杠杆，除非明确说明需要其他倍数并给出理由。
2. 可以更积极地捕捉机会，但依然要注意仓位控制，避免过度放大头寸。
3. 单笔基础仓位应按账户权益或可用保证金的固定比例给出：默认使用 15% 资金，依据信号强弱可在 10%-25% 区间调整，并在说明中写明比例与理由，避免出现过小名义价值（<2% 资金）的碎单。
4. 每笔 `place_order` 调用前，先调用 `get_instrument_specs` 并在输出中说明确认了最小单位/步进，确保数量合法。
5. 所有操作都需有对应理由并输出在响应中。"""


class StrategyAnalyzer:
    async def analyze(self, request: AnalysisRequest) -> AnalysisResponse:
        history = await conversation_manager.get_history("", limit=5)
        logger.info(
            "analysis_history_loaded",
            history_messages=len(history),
            symbol=request.symbol,
        )

        messages: list[ChatMessage] = [ChatMessage(role="system", content=_SYSTEM_PROMPT)]
        messages.extend(history)

        if request.symbol:
            messages.append(
                ChatMessage(
                    role="user",
                    content=(
                        f"本轮策略分析的唯一目标合约是 {request.symbol}。"
                        "所有行情查询、仓位评估与下单操作都必须聚焦该合约，禁止跨其他合约交易。"
                    ),
                )
            )

        messages.append(
            ChatMessage(
                role="user",
                content="请严格按照职责行事，若需要行情、账户或交易操作，请自行调用 MCP 工具。",
            )
        )

        event_collector = AgentEventCollector()

        tools_schema = get_tools_schema()

        async def _dispatch_chat() -> dict[str, Any]:
            logger.info(
                "deepseek_analysis_dispatch",
                history=len(history),
                symbol=request.symbol,
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
                    symbol=request.symbol,
                )
                try:
                    arguments = (
                        json.loads(arguments_raw) if isinstance(arguments_raw, str) else arguments_raw
                    ) or {}
                except json.JSONDecodeError as exc:
                    if isinstance(arguments_raw, str):
                        try:
                            arguments, _ = json.JSONDecoder().raw_decode(arguments_raw)
                        except json.JSONDecodeError:
                            raise ValueError(f"Invalid tool arguments: {exc}") from exc
                    else:
                        raise ValueError(f"Invalid tool arguments: {exc}") from exc

                result = await call_tool(name, arguments)
                logger.info(
                    "strategy_tool_executed",
                    tool=name,
                    arguments=arguments,
                    result_summary=str(result)[:800],
                    symbol=request.symbol,
                )

                event_collector.record_tool_call(name, arguments, result)

                messages.append(
                    ChatMessage(
                        role="tool",
                        content=json.dumps(result, ensure_ascii=False),
                        name=name,
                        tool_call_id=tool_call.get("id"),
                    )
                )

        result = await _dispatch_chat()

        max_iterations = 10
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
                    iteration=iteration,
                    symbol=request.symbol,
                )
                break

            logger.info(
                "strategy_tool_calls_detected",
                iteration=iteration,
                tool_count=len(tool_calls),
                symbol=request.symbol,
            )
            await _handle_tool_calls(tool_calls)

            if iteration == max_iterations:
                logger.warning(
                    "strategy_tool_loop_maxed",
                    max_iterations=max_iterations,
                    symbol=request.symbol,
                )
                break

            result = await _dispatch_chat()

        choice = result.get("choices", [{}])[0].get("message", {})
        summary = choice.get("content", "No analysis generated.")
        logger.info(
            "strategy_final_response",
            summary_preview=summary[:800] if isinstance(summary, str) else str(summary)[:800],
            symbol=request.symbol,
        )

        response = AnalysisResponse(
            summary=summary,
            created_at=datetime.now(tz=timezone.utc),
            symbol=request.symbol,
        )

        await conversation_manager.add_message(
            "",
            ChatMessage(role="assistant", content=summary),
        )

        logger.info("analysis_response_prepared", symbol=request.symbol)

        # 推送订单更新事件（如果有订单）
        for order_event in event_collector.order_events:
            await publish_task_result(
                {
                    "type": "order_update",
                    **order_event.to_dict(),
                }
            )

        logger.info(
            "strategy_analysis_completed",
            orders_count=len(event_collector.order_events),
        )

        return response


strategy_analyzer = StrategyAnalyzer()
