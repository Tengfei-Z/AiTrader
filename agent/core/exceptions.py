"""Custom exception hierarchy for the Agent service."""


class AgentError(Exception):
    """Base exception for Agent-level issues."""


class ConfigurationError(AgentError):
    """Raised when configuration is invalid or missing."""


class ExternalServiceError(AgentError):
    """Raised when an external dependency responds with an error."""


class RateLimitExceeded(ExternalServiceError):
    """Raised when the upstream API reports rate limiting."""


def is_retryable_external_error(exc: ExternalServiceError) -> bool:
    """Return whether an external error is likely to recover on retry."""

    if isinstance(exc, RateLimitExceeded):
        return True

    message = str(exc).lower()
    non_retryable_markers = (
        "401",
        "unauthorized",
        "50105",
        "passphrase incorrect",
        "business error",
    )
    if any(marker in message for marker in non_retryable_markers):
        return False

    retryable_markers = (
        "connecttimeout",
        "readtimeout",
        "writetimeout",
        "connecterror",
        "remoteprotocolerror",
        "timeoutexception",
        "transporterror",
        "too many requests",
        "429",
    )
    return any(marker in message for marker in retryable_markers)
