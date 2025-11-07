"""Configuration management for the Agent service."""

from functools import lru_cache
from pathlib import Path
from typing import Literal

from pydantic import AliasChoices, AnyHttpUrl, Field, SecretStr
from pydantic_settings import BaseSettings, SettingsConfigDict

_REPO_ROOT = Path(__file__).resolve().parents[2]
_AGENT_DIR = Path(__file__).resolve().parents[1]
# 把仓库根目录的 .env 作为默认来源，同时保留 agent 目录与运行时当前目录的回退
_ENV_FILE_CANDIDATES: tuple[str, ...] = (
    str(_REPO_ROOT / ".env"),
    str(_AGENT_DIR / ".env"),
    ".env",
)


class AgentSettings(BaseSettings):
    """Centralised configuration derived from environment variables."""

    app_env: Literal["development", "staging", "production"] = "development"
    log_level: Literal["DEBUG", "INFO", "WARNING", "ERROR"] = "INFO"
    log_file: str | None = Field(
        str(_REPO_ROOT / "log" / "agent.log"),
        description="日志文件路径，设为 None 或空字符串禁用文件输出",
    )

    agent_host: str = Field("0.0.0.0", description="FastAPI bind host")
    agent_port: int = Field(8001, description="FastAPI bind port")

    deepseek_api_key: SecretStr = Field(..., description="DeepSeek API key")
    deepseek_api_base: AnyHttpUrl = Field(
        "https://api.deepseek.com/", description="DeepSeek API endpoint"
    )

    okx_api_key: SecretStr = Field(
        ...,
        description="OKX API key",
        validation_alias=AliasChoices("OKX_API_KEY", "OKX_SIM_API_KEY"),
    )
    okx_secret_key: SecretStr = Field(
        ...,
        description="OKX secret key",
        validation_alias=AliasChoices("OKX_SECRET_KEY", "OKX_SIM_API_SECRET"),
    )
    okx_passphrase: SecretStr = Field(
        ...,
        description="OKX passphrase",
        validation_alias=AliasChoices("OKX_PASSPHRASE", "OKX_SIM_PASSPHRASE"),
    )
    okx_base_url: AnyHttpUrl = Field("https://www.okx.com", description="OKX REST base URL")

    model_config = SettingsConfigDict(
        env_file=_ENV_FILE_CANDIDATES,
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
    )


@lru_cache
def get_settings() -> AgentSettings:
    """Return a cached AgentSettings instance."""

    return AgentSettings()  # type: ignore[call-arg]


def resolved_env_file() -> str | None:
    """Return the first readable .env file from the candidate list."""

    for candidate in _ENV_FILE_CANDIDATES:
        path = Path(candidate).expanduser()
        if path.is_file():
            return str(path)
    return None


def env_file_candidates() -> tuple[str, ...]:
    """Expose configured env file search order for diagnostics."""

    return _ENV_FILE_CANDIDATES
