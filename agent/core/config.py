"""Configuration management for the Agent service."""

from functools import lru_cache
from typing import Literal

from pydantic import AnyHttpUrl, BaseSettings, Field, SecretStr


class AgentSettings(BaseSettings):
    """Centralised configuration derived from environment variables."""

    app_env: Literal["development", "staging", "production"] = "development"
    log_level: Literal["DEBUG", "INFO", "WARNING", "ERROR"] = "INFO"
    log_file: str | None = Field(
        "logs/agent.log",
        description="日志文件路径，设为 None 或空字符串禁用文件输出",
    )

    agent_host: str = Field("0.0.0.0", description="FastAPI bind host")
    agent_port: int = Field(8001, description="FastAPI bind port")

    deepseek_api_key: SecretStr = Field(..., description="DeepSeek API key")
    deepseek_api_base: AnyHttpUrl = Field(
        "https://api.deepseek.com/v1", description="DeepSeek API endpoint"
    )

    okx_api_key: SecretStr = Field(..., description="OKX API key")
    okx_secret_key: SecretStr = Field(..., description="OKX secret key")
    okx_passphrase: SecretStr = Field(..., description="OKX passphrase")
    okx_base_url: AnyHttpUrl = Field("https://www.okx.com", description="OKX REST base URL")

    sentry_dsn: str | None = Field(None, description="Optional Sentry DSN")

    class Config:
        env_file = ".env"
        env_file_encoding = "utf-8"
        case_sensitive = False


@lru_cache
def get_settings() -> AgentSettings:
    """Return a cached AgentSettings instance."""

    return AgentSettings()  # type: ignore[call-arg]
