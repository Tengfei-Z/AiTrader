"""Run the AiTrader Agent service locally.

This helper ensures a `.venv` exists under the agent directory. If the current
interpreter is not inside that virtualenv, the script will create it (when
missing) and re-exec itself using the virtualenv's Python.

Dependencies still need to be installed manually (e.g. `uv pip install -r ...`).
"""

from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

_AGENT_DIR = Path(__file__).resolve().parents[1]
_REPO_ROOT = _AGENT_DIR.parent
_VENV_DIR = _AGENT_DIR / ".venv"
_REEXEC_FLAG = "AITRADER_AGENT_IN_VENV"


def _running_inside_target_venv() -> bool:
    current_prefix = Path(sys.prefix).resolve()
    target = _VENV_DIR.resolve()
    return current_prefix == target or target in current_prefix.parents


def _venv_python_path() -> Path:
    if os.name == "nt":
        return _VENV_DIR / "Scripts" / "python.exe"
    return _VENV_DIR / "bin" / "python"


def ensure_virtualenv() -> None:
    if os.environ.get(_REEXEC_FLAG) == "1":
        return

    if _running_inside_target_venv():
        os.environ[_REEXEC_FLAG] = "1"
        return

    if not _VENV_DIR.exists():
        print(f"[run_agent] creating virtualenv at {_VENV_DIR}")
        subprocess.run([sys.executable, "-m", "venv", str(_VENV_DIR)], check=True)

    python_bin = _venv_python_path()
    if not python_bin.exists():
        raise RuntimeError(f"virtualenv python not found at {python_bin}")

    env = os.environ.copy()
    env[_REEXEC_FLAG] = "1"
    cmd = [
        str(python_bin),
        "-m",
        "agent.scripts.run_agent",
    ]
    subprocess.run(cmd, check=True, cwd=str(_REPO_ROOT), env=env)
    raise SystemExit(0)


def main() -> None:
    ensure_virtualenv()

    import uvicorn

    from agent.core.config import get_settings
    from agent.llm.main import app

    settings = get_settings()
    reload_enabled = settings.app_env == "development"
    if reload_enabled:
        uvicorn.run(
            "agent.llm.main:app",
            host=settings.agent_host,
            port=settings.agent_port,
            reload=True,
        )
    else:
        uvicorn.run(
            app,
            host=settings.agent_host,
            port=settings.agent_port,
            reload=False,
        )


if __name__ == "__main__":
    main()
