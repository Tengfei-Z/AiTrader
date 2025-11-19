"""Account-related MCP tools."""

from typing import Any

from ...core.okx_client import okx_client
from ..registry import mcp
from .utils import wrap_response


@mcp.tool(name="get_account_balance")
async def get_account_balance() -> dict[str, Any]:
    """
    查询账户余额信息。

    无需参数，返回全部币种的可用余额和冻结资金，可用于判断可用保证金与风险承受能力。
    """

    response = await okx_client.get_account_balance()
    return wrap_response(response)


@mcp.tool(name="get_positions")
async def get_positions(
    inst_type: str | None = None,
    inst_id: str | None = None,
) -> dict[str, Any]:
    """
    查询持仓信息，可按产品类型过滤。

    Parameters:
    - `inst_type`: 可选，`SWAP`/`SPOT` 等。若留空则返回所有类型。
    - `inst_id`: 可选，具体交易产品 ID（如 `BTC-USDT-SWAP`），可直接精确到单一合约。

    建议先调用此接口，明确当前方向（long/short）、数量与成本，用于决策是否加仓/减仓。
    """

    response = await okx_client.get_positions(inst_type=inst_type, inst_id=inst_id)
    return wrap_response(response)
