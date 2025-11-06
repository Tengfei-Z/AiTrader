import os

import pytest

from agent.core.config import AgentSettings


@pytest.fixture(autouse=True)
def _restore_env():
    original = os.environ.copy()
    yield
    os.environ.clear()
    os.environ.update(original)


def test_agent_settings_reads_env(monkeypatch):
    monkeypatch.setenv("DEEPSEEK_API_KEY", "deepseek-key")
    monkeypatch.setenv("OKX_API_KEY", "okx-key")
    monkeypatch.setenv("OKX_SECRET_KEY", "okx-secret")
    monkeypatch.setenv("OKX_PASSPHRASE", "okx-pass")

    settings = AgentSettings()

    assert settings.deepseek_api_key.get_secret_value() == "deepseek-key"
    assert settings.okx_api_key.get_secret_value() == "okx-key"
    assert settings.okx_secret_key.get_secret_value() == "okx-secret"
    assert settings.okx_passphrase.get_secret_value() == "okx-pass"
