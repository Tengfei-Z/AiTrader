"""Reusable HTTP client utilities."""

from collections.abc import AsyncIterator
from contextlib import asynccontextmanager

import httpx


@asynccontextmanager
async def async_http_client(
    base_url: str | None = None, timeout: float = 10.0
) -> AsyncIterator[httpx.AsyncClient]:
    """Yield a configured AsyncClient and close it afterwards."""

    async with httpx.AsyncClient(base_url=base_url, timeout=timeout) as client:
        yield client
