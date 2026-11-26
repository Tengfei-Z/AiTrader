from importlib import import_module

strategy_analyzer_module = import_module("agent.llm.services.strategy_analyzer")


def test_extract_system_prompt_from_source() -> None:
    source = """\n_OTHER = \"ignored\"\n_SYSTEM_PROMPT = \"live strategy text\"\n"""

    prompt = strategy_analyzer_module._extract_system_prompt_from_source(source)

    assert prompt == "live strategy text"


def test_system_prompt_fingerprint_is_stable() -> None:
    assert strategy_analyzer_module._system_prompt_fingerprint("abc") == "ba7816bf8f01"


def test_validate_place_order_blocks_direction_mismatch() -> None:
    result = strategy_analyzer_module._validate_place_order_against_message(
        "place_order",
        {"order": {"side": "buy", "ordType": "opponent", "sz": "1"}},
        "当前趋势明确下降，本轮决定做空。",
    )

    assert result is not None
    assert result["ok"] is False
    assert result["error"]["code"] == "direction_mismatch"


def test_validate_place_order_allows_reduce_only_close() -> None:
    result = strategy_analyzer_module._validate_place_order_against_message(
        "place_order",
        {"order": {"side": "buy", "reduceOnly": True, "ordType": "opponent", "sz": "1"}},
        "当前趋势明确下降，本轮决定做空。",
    )

    assert result is None


def test_execution_confirmation_summary_distinguishes_canceled_attempt() -> None:
    event = strategy_analyzer_module.OrderEvent(
        ord_id="ord-2",
        side="sell",
        action="开空",
        status="filled",
        filled_size="1",
        canceled_order_ids=["ord-1"],
        attempt_order_ids=["ord-1", "ord-2"],
    )

    summary = strategy_analyzer_module._execution_confirmation_summary([event])

    assert "开空最终确认成交：订单 ord-2" in summary
    assert "撤销的重试委托：ord-1" in summary
    assert "这些撤销委托不是最终成交单：ord-1" in summary


def test_collector_marks_direction_mismatch_as_rejected() -> None:
    collector = strategy_analyzer_module.AgentEventCollector()

    collector.record_tool_call(
        "place_order",
        {"order": {"side": "buy", "ordType": "opponent", "sz": "1"}},
        {"ok": False, "error": {"code": "direction_mismatch"}},
    )

    assert collector.order_events[0].status == "rejected"
    summary = strategy_analyzer_module._execution_confirmation_summary(collector.order_events)
    assert "最终下单失败" in summary


def test_execution_confirmation_warns_when_final_text_conflicts_with_actual_order() -> None:
    event = strategy_analyzer_module.OrderEvent(
        ord_id="ord-1",
        side="buy",
        action="开多",
        status="filled",
        filled_size="1",
    )

    summary = strategy_analyzer_module._execution_confirmation_summary(
        [event],
        "最终判断应该做空。",
    )

    assert "警告：模型最终文字方向与真实开仓方向不一致" in summary
