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


def _normalized_trigger_source(source: str | None) -> str:
    return (source or "").strip().lower().replace("-", "_")


def _is_manual_trigger(source: str | None) -> bool:
    return _normalized_trigger_source(source) == "manual"


def _is_smart_money_trigger(source: str | None) -> bool:
    normalized = _normalized_trigger_source(source)
    return normalized in {"smart_money", "smartmoney"}


def _with_smart_money_header(summary: str) -> str:
    header_prefix = "触发来源：Smart Money"
    header = f"{header_prefix} 🔥"
    if summary.startswith(header):
        return summary
    if summary.startswith(header_prefix):
        return summary.replace(header_prefix, header, 1)
    return f"{header}\n{summary}"


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

_SYSTEM_PROMPT = """
你是一个专业的加密货币交易 AI，负责独立分析市场、制定交易计划并执行策略。目标是最大化风险调整后的收益（如 Sharpe Ratio），同时保障账户稳健运行。

工作职责：
- 可激进开单，趋势明确后，可果断挂单开仓。
- 仅可交易白名单内的币种与合约。
- 当前测试固定使用 OKX 全仓保证金模式：所有 `place_order` 必须使用 `tdMode=cross`、`posSide=net`；禁止使用 `isolated`、`long` 或 `short` 作为下单持仓模式。
- 当前测试采用本金 all-in 但必须保留交易所手续费/保证金缓冲：当决定新开仓或反手建仓时，以账户可用 USDT 权益最多 95% 作为保证金预算，结合当前杠杆、合约面值和最小下单单位计算 `sz`；不得再使用小比例仓位。平仓/减仓单只按当前净仓位数量处理，不套用 all-in。
- 杠杆范围 5X - 25X。
- 分析前必须调用 `get_positions` 获取当前持仓；允许空仓等待。
- 严禁同一合约多空同持，反手必须先平掉原方向并再次确认仓位清零。
- 开仓时必须先设置好止损，止损应设置在支撑位上方或者下方，并且不可以超过2%
- 开仓时必须设置止盈，
- 可根据市场变化调整止盈止损
- 开仓必须使用挂单/对手价开仓；当你决定立即按对手价成交，调用 `place_order` 时使用 `ordType=opponent`，工具会自动用 buy=卖一价、sell=买一价转换为限价单，并在未成交时撤单按最新盘口重试，最终可降级市价成交剩余数量；当你有目标价位，请使用 `ordType=limit` 并提供 `px`。不要在同一轮里手动重复提交多笔相同对手价单。
- 挂单后可根据市场变化调整
- 在可在技术分析开仓点挂单开仓
- 技术分析基于k线形态为主，而非指标
- 综合3日内4小时k线、1日内1小时k线、12小时内5分钟k线综合分析
- 策略分析只考虑k线和交易量，不考虑指标
- 每次根据k线走势调整止盈（下一个技术止盈点）和止损（下一个技术止损点/确定利润点）
- 同时维持最多3个总仓位（仓位+挂单数小于等于3），3个仓位各使用最大保证金的1/3
- 对失效挂单（价格走势不符合预期的挂单）及时取消

可用 MCP 工具：
1. get_ticker：获取实时行情与盘口数据。
2. get_instrument_specs：查询合约规格（lotSz/minSz/tickSz），**每次下单前必须调用以确认下单数量是最小交易单位的整倍数**。
3. get_account_balance：查询账户状态与可用保证金。
4. get_positions：查看当前持仓与风险敞口。
5. place_order：执行交易（下单/平仓），需同步提供目标方向、价格、数量及杠杆。
6. cancel_order：撤销已提交的挂单。

输出要求（每次响应）：
1. 思考总结（≤200 字）：简述市场状况、持仓状态、下一步技术开仓价位。
2. 决策行动：如需操作，调用 MCP 工具并说明理由；

策略提示：
- 每次分析必须给出方向倾向（多/空/震荡），给出倾向后，必须在技术价位挂单开仓。不得空仓的同时不挂单
- 支撑位：附近止损
- 阻力位：附近止盈
- 顺势而为，尊重趋势变化。
- 下降趋势不做多，上涨趋势不做空 

其它要求：
1. 下单默认使用 5x 杠杆，可根据机会上调或者下调杠杆。
3. 仓位规则固定为本金 all-in：新开仓或反手后的目标仓位应尽量使用最多 95% 可用保证金预算，预留至少 5% 给手续费、价格波动与交易所保证金校验；必须先调用 `get_account_balance`、`get_ticker`、`get_instrument_specs`，按可用 USDT、杠杆、当前价格和合约规格计算合法 `sz`，并说明计算过程。若交易所限制、最小单位或风控原因导致无法使用到 95%，必须说明实际使用比例。
4. 每笔 `place_order` 调用前，先调用 `get_instrument_specs` 并在输出中说明确认了最小单位/步进，确保数量合法。
5. 严禁同一合约同时持有多头与空头对冲仓位；当前为全仓 net 模式，若需要反手，必须先依据最新 `get_positions` 结果，用相反 `side` 且 `reduceOnly=true` 的 `place_order` 彻底平掉原净仓位，并再次调用 `get_positions` 确认仓位已清零后，才能建立新的方向性仓位。
7. 若任一账户、持仓、行情或交易工具返回 `ok:false`，必须把它视为工具调用失败；不得假定账户状态、仓位状态或订单已经执行。若无法确认账户/持仓，禁止继续下单，只能说明上游不可用并给出保守建议。
"""


class StrategyAnalyzer:
    async def analyze(self, request: AnalysisRequest) -> AnalysisResponse:
        session_id = request.symbol or "__default__"
        history = await conversation_manager.get_history(session_id, limit=2)
        logger.info(
            "analysis_history_loaded",
            history_messages=len(history),
            session_id=session_id,
            symbol=request.symbol,
            trigger_source=request.trigger_source,
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

        if request.trigger_context:
            messages.append(
                ChatMessage(
                    role="user",
                    content=(
                        "本轮触发上下文如下，仅作为信号线索，不是强制下单指令。"
                        "请先用工具验证行情、账户与持仓，再决定观望、减仓、平仓或开仓："
                        f"{request.trigger_context}"
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

        async def _dispatch_chat(
            *,
            allow_tools: bool = True,
            temperature: float = 0.4,
        ) -> dict[str, Any]:
            logger.info(
                "deepseek_analysis_dispatch",
                history=len(history),
                symbol=request.symbol,
                trigger_source=request.trigger_source,
                count_against_budget=not _is_manual_trigger(request.trigger_source),
                allow_tools=allow_tools,
            )
            return await deepseek_client.chat_completion(
                messages,
                temperature=temperature,
                tools=tools_schema if allow_tools else None,
                tool_choice="auto" if allow_tools else None,
                count_against_budget=not _is_manual_trigger(request.trigger_source),
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
        hit_tool_iteration_limit = False

        max_iterations = 15
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
                hit_tool_iteration_limit = True
                logger.warning(
                    "strategy_tool_loop_maxed",
                    max_iterations=max_iterations,
                    symbol=request.symbol,
                )
                break

            result = await _dispatch_chat()

        if hit_tool_iteration_limit:
            messages.append(
                ChatMessage(
                    role="user",
                    content=(
                        "工具调用轮次已达到系统上限。现在禁止继续调用任何工具，请基于已经获取到的"
                        "行情、账户、持仓和下单结果，直接给出完整最终回复。必须包含：市场总结、"
                        "持仓/订单状态、已执行或未执行的决策、关键价格、数量、止盈止损、风险提示。"
                        "如果信息不足，也要明确说明哪些信息不足，不能再说“我需要继续获取”。"
                    ),
                )
            )
            result = await _dispatch_chat(allow_tools=False, temperature=0.2)

        choice = result.get("choices", [{}])[0].get("message", {})
        summary = choice.get("content", "No analysis generated.")
        if not isinstance(summary, str):
            summary = str(summary)
        if _is_smart_money_trigger(request.trigger_source):
            summary = _with_smart_money_header(summary)
        logger.info(
            "strategy_final_response",
            summary_preview=summary[:800] if isinstance(summary, str) else str(summary)[:800],
            symbol=request.symbol,
            trigger_source=request.trigger_source,
        )

        response = AnalysisResponse(
            summary=summary,
            created_at=datetime.now(tz=timezone.utc),
            symbol=request.symbol,
        )

        await conversation_manager.add_message(
            session_id,
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
