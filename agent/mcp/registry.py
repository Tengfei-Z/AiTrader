"""Shared FastMCP registry instance."""

from fastmcp import FastMCP

mcp = FastMCP("aitrader-agent")

__all__ = ["mcp"]
