"""Custom exception hierarchy for the Agent service."""


class AgentError(Exception):
    """Base exception for Agent-level issues."""


class ConfigurationError(AgentError):
    """Raised when configuration is invalid or missing."""


class ExternalServiceError(AgentError):
    """Raised when an external dependency responds with an error."""


class RateLimitExceeded(ExternalServiceError):
    """Raised when the upstream API reports rate limiting."""
