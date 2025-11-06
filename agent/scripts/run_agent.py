"""Run the AiTrader Agent service locally."""

import uvicorn

from agent.core.config import get_settings
from agent.llm.main import app


def main() -> None:
    settings = get_settings()
    uvicorn.run(
        app,
        host=settings.agent_host,
        port=settings.agent_port,
        reload=settings.app_env == "development",
    )


if __name__ == "__main__":
    main()
