"""API-key authentication, RBAC and source-scoped authorization for Escenario 3."""

from dataclasses import dataclass
from hmac import compare_digest
from typing import Annotated, Literal

from fastapi import Header, HTTPException, Request

from .config import Settings


@dataclass(frozen=True)
class Principal:
    role: Literal["producer", "analyst"]
    allowed_sources: frozenset[str] | None = None


_ALLOWED = frozenset(
    {
        ("producer", "events:create"),
        ("analyst", "events:read"),
        ("analyst", "analysis:read"),
    }
)


def _unauthenticated() -> HTTPException:
    return HTTPException(status_code=401, detail="Invalid or missing API key")


def _resolve_settings(request: Request | None = None) -> Settings:
    if request is not None:
        explicit = getattr(request.app.state, "explicit_settings", None)
        if explicit is not None:
            return explicit
    return Settings.from_env()


def _authenticate(
    api_key: str | None, settings: Settings | None = None
) -> Principal:
    cfg = settings if settings is not None else Settings.from_env()
    if cfg.auth_config_invalid:
        raise _unauthenticated()

    producer_key = (
        cfg.event_producer_key.get_secret_value()
        if cfg.event_producer_key is not None
        else None
    )
    analyst_key = (
        cfg.event_analyst_key.get_secret_value()
        if cfg.event_analyst_key is not None
        else None
    )

    # Both base credentials must exist and be distinct. Any partial or ambiguous
    # configuration (including scoped keys colliding with global keys) fails closed.
    if (
        not producer_key
        or not producer_key.strip()
        or not analyst_key
        or not analyst_key.strip()
        or compare_digest(producer_key, analyst_key)
    ):
        raise _unauthenticated()

    scoped_entries = [
        (source, secret.get_secret_value())
        for source, secret in cfg.event_producer_source_keys.items()
    ]
    for _, scoped_secret in scoped_entries:
        if (
            not scoped_secret.strip()
            or compare_digest(scoped_secret, producer_key)
            or compare_digest(scoped_secret, analyst_key)
        ):
            raise _unauthenticated()

    if not api_key or not api_key.strip():
        raise _unauthenticated()

    producer_match = compare_digest(api_key, producer_key)
    analyst_match = compare_digest(api_key, analyst_key)
    matched_sources = frozenset(
        source
        for source, scoped_secret in scoped_entries
        if compare_digest(api_key, scoped_secret)
    )

    if producer_match:
        return Principal(role="producer", allowed_sources=None)
    if analyst_match:
        return Principal(role="analyst", allowed_sources=None)
    if matched_sources:
        return Principal(role="producer", allowed_sources=matched_sources)
    raise _unauthenticated()


def require_producer(
    request: Request = None,  # type: ignore[assignment]
    x_api_key: Annotated[str | None, Header(alias="X-API-Key")] = None,
) -> Principal:
    """FastAPI dependency for event creation routes."""
    principal = _authenticate(x_api_key, _resolve_settings(request))
    if principal.role != "producer":
        raise HTTPException(status_code=403, detail="Forbidden")
    return principal


def require_analyst(
    request: Request = None,  # type: ignore[assignment]
    x_api_key: Annotated[str | None, Header(alias="X-API-Key")] = None,
) -> Principal:
    """FastAPI dependency for event and analysis read routes."""
    principal = _authenticate(x_api_key, _resolve_settings(request))
    if principal.role != "analyst":
        raise HTTPException(status_code=403, detail="Forbidden")
    return principal


def ensure_permission(
    principal: Principal,
    action: Literal["events:create", "events:read", "analysis:read"],
) -> None:
    """Enforce an operation's permission in its owning service function."""
    if (principal.role, action) not in _ALLOWED:
        raise HTTPException(status_code=403, detail="Forbidden")


def ensure_source_access(principal: Principal, source: str) -> None:
    """Enforce lateral source isolation when a producer credential is source-scoped."""
    ensure_permission(principal, "events:create")
    if (
        principal.allowed_sources is not None
        and source not in principal.allowed_sources
    ):
        raise HTTPException(status_code=403, detail="Forbidden")
