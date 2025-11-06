# AiTrader Agent Service

This package hosts the Python implementation of the AI Agent service that mediates
between the Rust backend and external LLM providers. The service encapsulates:

- A FastAPI HTTP interface for AI conversations and analytical workflows.
- A DeepSeek client with FastMCP tool integration to orchestrate function calling.
- Direct OKX REST interactions exposed as FastMCP tools.
- Shared infrastructure components for configuration, logging, and HTTP access.

## Getting Started

1. Create a virtual environment and install dependencies:

```bash
uv venv
source .venv/bin/activate
uv pip install -r requirements.txt
```

2. Copy `.env.example` to `.env` and populate credentials.

3. Run the development server:

```bash
uvicorn llm.main:app --reload --host 0.0.0.0 --port 8001
# or
python -m agent.scripts.run_agent
```

## Project Layout

The repository layout is documented comprehensively in `DESIGN.md`. Components are
grouped into:

- `core/` for reusable utilities and infrastructure.
- `llm/` for the FastAPI application and service layer.
- `mcp/` for FastMCP tool definitions.
- `scripts/` for operational helpers (deployment, migrations, etc.).

## Status

This repository currently contains scaffolding for the Agent service. Implementations
of MCP tools, DeepSeek integration, and OKX adapters are in progress. Refer to the
phase checklist in `DESIGN.md` before extending functionality.

## Next Steps

- Integrate FastMCP tool definitions in `mcp/` and expose schema loading to the DeepSeek client.
- Flesh out OKX request/response models and error handling, adding unit tests in `tests/`.
- Implement persistence or cache-backed conversation storage for multi-instance deployments.
- Add CI automation for linting (`ruff`), type checking (`mypy`), and async API tests.
- Expand analysis workflows with richer OKX data sources (funding, greeks, liquidity metrics).
- Consider moving conversation history to Redis (see `conversation_manager`) for horizontal scaling.

## Testing

```bash
cd agent
uv pip install -r requirements.txt
uv pip install -r requirements-dev.txt
pytest
```

## Features

- FastAPI endpoints for health, chat, and analysis requests.
- DeepSeek client with FastMCP tool orchestration and retry handling.
- OKX client wrappers surfaced as FastMCP tools (market data, balances, orders).
- In-memory conversation management with unit tests validating retention and tool flows.
- Startup hooks warm the FastMCP schema cache; integration tests cover chat/analysis endpoints.
