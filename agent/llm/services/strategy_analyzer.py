"""Domain-specific analysis helpers."""

from __future__ import annotations

import ast
import hashlib
import json
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from ...core.event_manager import publish_task_result
from ...core.logging_config import get_logger
from ...mcp.server import call_tool, get_tools_schema
from ..schemas.analysis import AnalysisRequest, AnalysisResponse
from ..schemas.chat import ChatMessage
from .conversation_manager import conversation_manager
from .deepseek_client import deepseek_client

logger = get_logger(__name__)
_SYSTEM_PROMPT_SOURCE_PATH = Path(__file__)


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
    ord_id: str | None
    side: str | None
    action: str
    status: str
    filled_size: str | None = None
    canceled_order_ids: list[str] | None = None
    attempt_order_ids: list[str] | None = None

    def to_dict(self) -> dict[str, Any]:
        payload: dict[str, Any] = {}
        if self.ord_id:
            payload["ordId"] = self.ord_id
        payload["status"] = self.status
        payload["action"] = self.action
        if self.side:
            payload["side"] = self.side
        return payload


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

        order = _extract_order_arguments(arguments)
        confirmation = result.get("aitraderConfirmation") if isinstance(result, dict) else None
        entry = next((item for item in payloads if isinstance(item, dict)), {})
        ord_id = (
            confirmation.get("finalOrdId") if isinstance(confirmation, dict) else None
        ) or entry.get("ordId") or entry.get("orderId") or entry.get("order_id") or entry.get("tradeId")

        attempts = confirmation.get("attempts", []) if isinstance(confirmation, dict) else []
        attempt_order_ids = [
            str(attempt.get("ordId"))
            for attempt in attempts
            if isinstance(attempt, dict) and attempt.get("ordId")
        ]
        if not ord_id and attempt_order_ids:
            ord_id = attempt_order_ids[-1]

        if not ord_id:
            logger.info(
                "strategy_place_order_no_ord_id",
                tool=name,
                payload=entry,
            )

        failed_result = isinstance(result, dict) and result.get("ok") is False
        event = OrderEvent(
            ord_id=str(ord_id) if ord_id else None,
            side=str(order.get("side")) if order.get("side") else None,
            action=_describe_order_action(order),
            status=(
                "rejected"
                if failed_result
                else str(confirmation.get("status") if isinstance(confirmation, dict) else "submitted")
            ),
            filled_size=(
                str(confirmation.get("totalFilledSz"))
                if isinstance(confirmation, dict) and confirmation.get("totalFilledSz") is not None
                else None
            ),
            canceled_order_ids=[
                str(value)
                for value in (confirmation.get("canceledOrdIds") if isinstance(confirmation, dict) else [])
            ],
            attempt_order_ids=attempt_order_ids,
        )
        self.order_events.append(event)
        logger.info(
            "strategy_place_order_recorded",
            tool=name,
            ord_id=event.ord_id,
            status=event.status,
            action=event.action,
            filled_size=event.filled_size,
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


def _extract_order_arguments(arguments: dict[str, Any]) -> dict[str, Any]:
    order = arguments.get("order") if isinstance(arguments, dict) else None
    return order if isinstance(order, dict) else arguments


def _truthy(value: Any) -> bool:
    if isinstance(value, bool):
        return value
    if isinstance(value, str):
        return value.strip().lower() in {"1", "true", "yes"}
    return bool(value)


def _describe_order_action(order: dict[str, Any]) -> str:
    side = str(order.get("side") or "").lower()
    reduce_only = _truthy(order.get("reduceOnly"))
    if reduce_only and side == "buy":
        return "平空"
    if reduce_only and side == "sell":
        return "平多"
    if side == "buy":
        return "开多"
    if side == "sell":
        return "开空"
    return "下单"


def _message_direction_intent(text: str) -> str | None:
    lowered = text.lower()
    bearish = any(keyword in lowered for keyword in ("做空", "开空", "空头", "看跌", "short", "sell"))
    bullish = any(keyword in lowered for keyword in ("做多", "开多", "多头", "看涨", "long", "buy"))
    if bearish and not bullish:
        return "sell"
    if bullish and not bearish:
        return "buy"
    return None


def _validate_place_order_against_message(
    name: str | None, arguments: dict[str, Any], assistant_text: str
) -> dict[str, Any] | None:
    if name != "place_order":
        return None
    order = _extract_order_arguments(arguments)
    if _truthy(order.get("reduceOnly")):
        return None
    intended_side = _message_direction_intent(assistant_text or "")
    actual_side = str(order.get("side") or "").lower()
    if intended_side and actual_side and intended_side != actual_side:
        return {
            "ok": False,
            "error": {
                "code": "direction_mismatch",
                "tool": "place_order",
                "message": (
                    f"模型文字方向为 {intended_side}，但开仓委托 side={actual_side}。"
                    "系统已阻断该下单，需重新分析并用一致方向提交。"
                ),
                "retryable": False,
            },
        }
    return None


def _final_direction_warning(summary: str, events: list[OrderEvent]) -> str:
    intended_side = _message_direction_intent(summary)
    if not intended_side:
        return ""
    opening_events = [event for event in events if event.action in {"开多", "开空"}]
    mismatched = [
        event
        for event in opening_events
        if event.side and event.side != intended_side and event.status != "rejected"
    ]
    if not mismatched:
        return ""
    details = ", ".join(
        f"{event.action} {event.ord_id or '无订单号'} status={event.status}"
        for event in mismatched
    )
    return f"\n- 警告：模型最终文字方向与真实开仓方向不一致，真实执行为：{details}。"


def _execution_confirmation_summary(events: list[OrderEvent], final_summary: str = "") -> str:
    if not events:
        return ""

    lines = ["", "---", "系统执行确认："]
    for event in events:
        filled = event.filled_size or "未知"
        if event.status == "filled" and event.ord_id:
            lines.append(
                f"- {event.action}最终确认成交：订单 {event.ord_id}，成交数量 {filled}。"
            )
        elif event.status in {"not_filled", "canceled", "rejected"}:
            order_text = f"订单 {event.ord_id}" if event.ord_id else "本次委托"
            lines.append(f"- {event.action}{order_text}未成交，最终下单失败。")
        else:
            order_text = f"订单 {event.ord_id}" if event.ord_id else "本次委托"
            lines.append(f"- {event.action}{order_text}状态为 {event.status}，成交数量 {filled}，需要以后续持仓同步为准。")

        if event.canceled_order_ids:
            lines.append(f"  撤销的重试委托：{', '.join(event.canceled_order_ids)}。")
        if event.attempt_order_ids and event.ord_id:
            canceled = set(event.canceled_order_ids or [])
            unfilled_attempts = [oid for oid in event.attempt_order_ids if oid in canceled]
            if unfilled_attempts:
                lines.append(f"  注意：这些撤销委托不是最终成交单：{', '.join(unfilled_attempts)}。")
    warning = _final_direction_warning(final_summary, events)
    if warning:
        lines.append(warning)
    return "\n".join(lines)

_SYSTEM_PROMPT = """
你是一个专业的加密货币交易 AI，负责独立分析市场、制定交易计划并执行策略。目标是最大化风险调整后的收益（如 Sharpe Ratio），同时保障账户稳健运行。

工作职责：
- 开仓时必须先设置好止损，止损应设置在支撑位上方或者下方，止损价格要求：损失不可以超过本金2%
- 必须分析当前趋势、分别按照15M、1H和4H k线(至少分析前5天)确定当前趋势，必须顺趋势交易（上升趋势只能做多，下降趋势只能做空）
- 可根据市场变化调整止盈止损（盈利时优先锁定一部分利润/保本，然后让利润奔跑，在盈利时尽可能持仓）
- 可激进开单，趋势明确后，可果断委托开仓。
- 委托/交易时，先检查有几个持仓，和几个委托。持仓+委托数必须小于等于1个。
- 委托/交易时使用总本金的全部。
- 你可以调整/取消已有的委托。
- 开仓、止损/止盈、时，必须明确当前开仓基于什么时间k线（15M、1H和4H ）
- 开仓、止损/止盈、时在5m k线寻找入场机会，在1H、4H k线上寻找止盈点。
- 用技术分析开仓点委托开仓，委托价格应该高明，比如66000—>65950,66000->66050.
- 策略分析只考虑k线和交易量，不考虑指标
- 每次根据k线走势可以调整止盈价格（下一个技术止盈点）和止损价格（下一个技术止损点/确定利润点）
- 对失效委托（价格走势不符合预期的委托）及时取消


- 若已有持仓缺少止盈/止损，必须调用 `set_position_tpsl` 为现有净仓位设置保护单；不要用 `place_order.attachAlgoOrds` 给已有持仓补止盈止损，`attachAlgoOrds` 只用于新开仓主单成交后的附加止盈止损。
- 仅可交易白名单内的币种与合约。
- 当前测试固定使用 OKX 全仓保证金模式：所有 `place_order` 必须使用 `tdMode=cross`、`posSide=net`；禁止使用 `isolated`、`long` 或 `short` 作为下单持仓模式。
- 当前测试采用本金 all-in 但必须保留交易所手续费/保证金缓冲：当决定新开仓或反手建仓时，以账户可用 USDT 权益最多 95% 作为保证金预算，结合当前杠杆、合约面值和最小下单单位计算 `sz`；不得再使用小比例仓位。平仓/减仓单只按当前净仓位数量处理，不套用 all-in。
- 杠杆范围 5X - 25X。
- 分析前必须调用 `get_positions` 获取当前持仓；
- 分析前必须调用 `get_open_orders` 查询目标合约当前未成交普通委托；
- 严禁同一合约多空同持，反手必须先平掉原方向并再次确认仓位清零。
- 开仓时必须设置止盈，
- 开仓必须使用委托/对手价开仓；当你决定立即按对手价成交，调用 `place_order` 时使用 `ordType=opponent`，工具会自动用 buy=卖一价、sell=买一价转换为限价单，并在未成交时撤单按最新盘口重试，最终可降级市价成交剩余数量；当你有目标价位，请使用 `ordType=limit` 并提供 `px`。不要在同一轮里手动重复提交多笔相同对手价单。


可用 MCP 工具：
1. get_ticker：获取实时行情与盘口数据。
2. get_instrument_specs：查询合约规格（lotSz/minSz/tickSz），**每次下单前必须调用以确认下单数量是最小交易单位的整倍数**。
3. get_account_balance：查询账户状态与可用保证金。
4. get_positions：查看当前持仓与风险敞口。
5. get_open_orders：查询当前未成交普通委托，Smart Money 触发时必须用它检查遗留委托。
6. place_order：执行交易（下单/平仓），需同步提供目标方向、价格、数量及杠杆。
7. set_position_tpsl：为已有净持仓设置平仓止盈/止损算法单。
8. cancel_order：撤销单笔已提交普通委托。
9. cancel_open_orders：批量撤销指定合约当前未成交普通委托。

输出要求（每次响应）：
1. 思考总结（≤200 字）
2. 明确当前趋势、以及趋势基于什么时间k线（15M、1H和4H ）得出



其它要求：
1. 下单默认使用 5x 杠杆，可根据机会上调或者下调杠杆。
3. 仓位规则固定为本金 all-in：新开仓或反手后的目标仓位应尽量使用最多 95% 可用保证金预算，预留至少 5% 给手续费、价格波动与交易所保证金校验；必须先调用 `get_account_balance`、`get_ticker`、`get_instrument_specs`，按可用 USDT、杠杆、当前价格和合约规格计算合法 `sz`，并说明计算过程。若交易所限制、最小单位或风控原因导致无法使用到 95%，必须说明实际使用比例。
4. 每笔 `place_order` 调用前，先调用 `get_instrument_specs` 并在输出中说明确认了最小单位/步进，确保数量合法。
5. 严禁同一合约同时持有多头与空头对冲仓位；当前为全仓 net 模式，若需要反手，必须先依据最新 `get_positions` 结果，用相反 `side` 且 `reduceOnly=true` 的 `place_order` 彻底平掉原净仓位，并再次调用 `get_positions` 确认仓位已清零后，才能建立新的方向性仓位。
6. 若 `get_open_orders` 显示存在同合约遗留普通委托，在新增开仓委托前必须明确判断保留或撤销；若委托方向/价格已经失效，优先调用 `cancel_open_orders` 或逐笔调用 `cancel_order`。撤单工具返回 `ok:false` 或批量撤单存在 failed 时，禁止继续提交新的同方向或冲突开仓委托。
7. 若 `get_positions` 显示已有仓位但 `closeOrderAlgo` 为空，或你判断当前持仓需要调整止盈/止损，调用 `set_position_tpsl`。净多仓保护单使用 `side=sell`，净空仓保护单使用 `side=buy`；同时设置止盈和止损时提供 `tpTriggerPx` 与 `slTriggerPx`。
8. 若任一账户、持仓、行情或交易工具返回 `ok:false`，必须把它视为工具调用失败；不得假定账户状态、仓位状态或订单已经执行。若无法确认账户/持仓，禁止继续下单，只能说明上游不可用并给出保守建议。
"""


_SYSTEM_PROMPT_FALLBACK = _SYSTEM_PROMPT


def _extract_system_prompt_from_source(source: str) -> str | None:
    module = ast.parse(source)
    for node in module.body:
        if not isinstance(node, ast.Assign):
            continue
        if not any(
            isinstance(target, ast.Name) and target.id == "_SYSTEM_PROMPT"
            for target in node.targets
        ):
            continue
        value = ast.literal_eval(node.value)
        if isinstance(value, str) and value.strip():
            return value
    return None


def _load_system_prompt() -> str:
    try:
        source = _SYSTEM_PROMPT_SOURCE_PATH.read_text(encoding="utf-8")
        prompt = _extract_system_prompt_from_source(source)
    except Exception as exc:  # noqa: BLE001 - fallback keeps analysis available
        logger.warning(
            "system_prompt_reload_failed",
            source=str(_SYSTEM_PROMPT_SOURCE_PATH),
            error=str(exc),
        )
        return _SYSTEM_PROMPT_FALLBACK

    if not prompt:
        logger.warning(
            "system_prompt_reload_empty",
            source=str(_SYSTEM_PROMPT_SOURCE_PATH),
        )
        return _SYSTEM_PROMPT_FALLBACK
    return prompt


def _system_prompt_fingerprint(prompt: str) -> str:
    return hashlib.sha256(prompt.encode("utf-8")).hexdigest()[:12]


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

        system_prompt = _load_system_prompt()
        logger.info(
            "analysis_system_prompt_loaded",
            source=str(_SYSTEM_PROMPT_SOURCE_PATH),
            fingerprint=_system_prompt_fingerprint(system_prompt),
            length=len(system_prompt),
            trigger_source=request.trigger_source,
            symbol=request.symbol,
        )

        messages: list[ChatMessage] = [ChatMessage(role="system", content=system_prompt)]
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

        if _is_smart_money_trigger(request.trigger_source):
            target = request.symbol or "本轮目标合约"
            messages.append(
                ChatMessage(
                    role="user",
                    content=(
                        f"本轮是 Smart Money 触发。处理 {target} 前，必须先调用 "
                        "`get_open_orders` 查询该合约未成交普通委托；若存在上一轮遗留、"
                        "与 Smart Money 信号方向冲突、价格明显偏离当前技术结构，或会导致"
                        "持仓+委托数超过 3 的普通委托，必须先调用 `cancel_open_orders` "
                        "或逐笔 `cancel_order` 撤销。只有确认撤单成功或确认委托仍有效后，"
                        "才能继续新增开仓委托；撤单失败时不要再提交新的冲突委托。"
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

                result = _validate_place_order_against_message(
                    name, arguments, assistant_message.content or ""
                )
                if result is None:
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
        summary = f"{summary}{_execution_confirmation_summary(event_collector.order_events, summary)}"
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
