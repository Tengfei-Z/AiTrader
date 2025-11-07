"""Integration test for DeepSeek using the OpenAI sync client."""

from __future__ import annotations

import os
from pathlib import Path

import pytest
from dotenv import load_dotenv
from openai import OpenAI


_ENV_LOADED = False


def _ensure_env_loaded() -> None:
    global _ENV_LOADED
    if _ENV_LOADED:
        return
    repo_root = Path(__file__).resolve().parents[2]
    env_path = repo_root / ".env"
    if env_path.exists():
        load_dotenv(env_path)
    _ENV_LOADED = True


def _build_sync_client() -> OpenAI:
    # api_key = os.getenv("DEEPSEEK_API_KEY")
    api_key="sk-2db2fec6c10a4475ac71fb0fc99f5d40"
    base_url = os.getenv("DEEPSEEK_API_BASE", "https://api.deepseek.com")

    if not api_key:
        pytest.skip("DEEPSEEK_API_KEY not configured; skipping live DeepSeek test")

    base_url = base_url.rstrip("/")
    if not base_url.endswith("/v1"):
        base_url = f"{base_url}/v1"

    return OpenAI(api_key=api_key, base_url=base_url)


def test_deepseek_can_add_two_numbers_sync():
    _ensure_env_loaded()
    client = _build_sync_client()

    response = client.chat.completions.create(
        model="deepseek-chat",
        messages=[
            {"role": "system", "content": "You are a precise calculator."},
            {"role": "user", "content": "1+1=?"},
        ],
        temperature=0,
        max_tokens=32,
    )

    answer = response.choices[0].message.content.strip().lower()

    assert "2" in answer or "two" in answer
