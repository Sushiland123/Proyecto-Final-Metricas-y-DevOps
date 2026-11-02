"""Composición HTTP y sondas de salud del servicio de ingesta y análisis de eventos."""

from fastapi import FastAPI, Request
from sqlalchemy import create_engine, text
from sqlalchemy.engine import make_url
from sqlalchemy.exc import ArgumentError, SQLAlchemyError

from event_service.api import router
from event_service.config import Settings
from event_service.contracts import StorageUnavailable
from event_service.errors import register_handlers


def create_app(settings: Settings | None = None) -> FastAPI:
    """Crea la app; el engine no abre conexiones hasta ejecutar readiness.

    ``app.state.settings`` y ``app.state.engine`` son puntos de integración
    para autorización y persistencia. El engine se comparte por proceso; no se
    crea uno en cada solicitud de salud.
    """
    resolved = settings if settings is not None else Settings.from_env()
    app = FastAPI(title="Event service")
    app.state.settings = resolved
    app.state.explicit_settings = settings
    app.state.engine = None
    if resolved.database_url is not None:
        try:
            url = make_url(resolved.database_url)
            if url.drivername != "postgresql+psycopg":
                raise ArgumentError("Expected PostgreSQL with psycopg")
            app.state.engine = create_engine(
                url,
                pool_pre_ping=True,
                connect_args={
                    "connect_timeout": 3,
                    "options": "-c statement_timeout=3000",
                },
            )
        except SQLAlchemyError:
            # Una URL mal configurada tampoco debe impedir liveness ni salir
            # al cliente en una excepción de arranque; readiness marcará 503.
            pass

    app.include_router(router)
    register_handlers(app)

    @app.get("/health/live")
    def live() -> dict[str, str]:
        return {"status": "alive"}

    @app.get("/health/ready")
    def ready(request: Request) -> dict[str, str]:
        engine = request.app.state.engine
        if engine is None:
            raise StorageUnavailable

        try:
            with engine.connect() as connection:
                connection.execute(text("SELECT 1"))
        except SQLAlchemyError as exc:
            raise StorageUnavailable from exc

        return {"status": "ready"}

    return app


app = create_app()
