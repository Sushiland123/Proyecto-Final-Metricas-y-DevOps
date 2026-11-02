"""Configuración de la aplicación para el Escenario 3, sin credenciales en código."""

import os
import re
from collections.abc import Mapping
from typing import Self

from pydantic import BaseModel, ConfigDict, Field, SecretStr

_SOURCE_IDENTIFIER = re.compile(r"[A-Za-z0-9][A-Za-z0-9_-]{0,63}\Z", re.ASCII)


def _parse_source_keys(raw: str | None) -> tuple[dict[str, SecretStr], bool]:
    """Parsea ``EVENT_PRODUCER_SOURCE_KEYS`` con formato ``fuente:clave,fuente2:clave2``.

    Si la variable contiene entradas mal formadas, fuentes inválidas, claves
    vacías o fuentes repetidas, marca la configuración como inválida para que el
    módulo de autenticación falle cerrado (401) sin exponer secretos.
    """
    if raw is None or not raw.strip():
        return {}, False

    parsed: dict[str, SecretStr] = {}
    for entry in raw.split(","):
        item = entry.strip()
        if not item:
            return {}, True
        separator = ":" if ":" in item else ("=" if "=" in item else None)
        if separator is None:
            return {}, True
        source, secret = item.split(separator, 1)
        source = source.strip()
        secret = secret.strip()
        if (
            not source
            or not secret
            or _SOURCE_IDENTIFIER.fullmatch(source) is None
            or source in parsed
        ):
            return {}, True
        parsed[source] = SecretStr(secret)
    return parsed, False


def _bounded_int(
    source: Mapping[str, str],
    name: str,
    default: int,
    *,
    minimum: int,
    maximum: int,
) -> int:
    raw = source.get(name)
    if raw is None or not raw.strip():
        return default
    try:
        value = int(raw.strip())
    except ValueError:
        return default
    if minimum <= value <= maximum:
        return value
    return default


class Settings(BaseModel):
    """Configuración opcional hasta que el entorno provea DB y claves de acceso.

    Ausencia de una clave o ambigüedad entre credenciales no concede acceso: el
    módulo de autorización rechaza el acceso protegido fallando cerrado.
    """

    model_config = ConfigDict(frozen=True, hide_input_in_errors=True)

    database_url: str | None = Field(default=None, repr=False)
    event_producer_key: SecretStr | None = Field(default=None, repr=False)
    event_analyst_key: SecretStr | None = Field(default=None, repr=False)
    event_producer_source_keys: dict[str, SecretStr] = Field(
        default_factory=dict, repr=False
    )
    auth_config_invalid: bool = Field(default=False)
    db_connect_timeout_seconds: int = Field(default=3, ge=1, le=60)
    db_statement_timeout_ms: int = Field(default=5000, ge=100, le=60000)
    db_read_max_retries: int = Field(default=1, ge=0, le=5)
    db_read_retry_backoff_ms: int = Field(default=50, ge=0, le=2000)

    @classmethod
    def from_env(cls, environ: Mapping[str, str] | None = None) -> Self:
        """Lee DATABASE_URL, claves y parámetros de resiliencia desde el entorno.

        Permite inyectar un mapeo aislado en pruebas; ``None`` usa el entorno
        real. Cadenas vacías o compuestas solo por espacios equivalen a ausente.
        """
        source = os.environ if environ is None else environ

        def present(name: str) -> str | None:
            value = source.get(name)
            return value if value and value.strip() else None

        source_keys, invalid_sources = _parse_source_keys(
            source.get("EVENT_PRODUCER_SOURCE_KEYS")
        )

        return cls(
            database_url=present("DATABASE_URL"),
            event_producer_key=present("EVENT_PRODUCER_KEY"),
            event_analyst_key=present("EVENT_ANALYST_KEY"),
            event_producer_source_keys=source_keys,
            auth_config_invalid=invalid_sources,
            db_connect_timeout_seconds=_bounded_int(
                source, "DB_CONNECT_TIMEOUT_SECONDS", 3, minimum=1, maximum=60
            ),
            db_statement_timeout_ms=_bounded_int(
                source, "DB_STATEMENT_TIMEOUT_MS", 5000, minimum=100, maximum=60000
            ),
            db_read_max_retries=_bounded_int(
                source, "DB_READ_MAX_RETRIES", 1, minimum=0, maximum=5
            ),
            db_read_retry_backoff_ms=_bounded_int(
                source, "DB_READ_RETRY_BACKOFF_MS", 50, minimum=0, maximum=2000
            ),
        )
