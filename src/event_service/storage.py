"""PostgreSQL persistence adapter for Escenario 3 events and analytical queries."""

import time
from collections.abc import Callable, Iterator, Sequence
from dataclasses import dataclass
from datetime import UTC, datetime
from functools import lru_cache
from typing import Any, Literal, TypeVar
from uuid import UUID, uuid4

from fastapi import Request
from sqlalchemy import (
    Column,
    DateTime,
    Index,
    MetaData,
    Numeric,
    String,
    Table,
    UniqueConstraint,
    case,
    cast,
    create_engine,
    func,
    insert,
    select,
)
from sqlalchemy.dialects.postgresql import JSONB
from sqlalchemy.dialects.postgresql import UUID as PG_UUID
from sqlalchemy.engine import make_url
from sqlalchemy.exc import (
    ArgumentError,
    IntegrityError,
    OperationalError,
    SQLAlchemyError,
)
from sqlalchemy.orm import Session, sessionmaker

from .config import Settings
from .contracts import DuplicateEvent, StorageUnavailable

T = TypeVar("T")

metadata = MetaData()
events = Table(
    "events",
    metadata,
    Column("id", PG_UUID(as_uuid=True), primary_key=True),
    Column("source", String(64), nullable=False),
    Column("external_id", String(128), nullable=False),
    Column("type", String(64), nullable=False),
    Column("occurred_at", DateTime(timezone=True), nullable=False),
    Column("ingested_at", DateTime(timezone=True), nullable=False),
    Column("payload", JSONB, nullable=False),
    UniqueConstraint("source", "external_id", name="uq_events_source_external_id"),
    Index("ix_events_occurred_at_type", "occurred_at", "type"),
    Index("ix_events_occurred_at_source", "occurred_at", "source"),
)


@dataclass(frozen=True)
class StoredEvent:
    id: UUID
    source: str
    external_id: str
    type: str
    occurred_at: datetime
    ingested_at: datetime
    payload: dict


def _utc(value: datetime) -> datetime:
    if value.tzinfo is None or value.utcoffset() is None:
        raise ValueError("Timestamp must include a timezone")
    return value.astimezone(UTC)


def _stored_event(row: Any) -> StoredEvent:
    return StoredEvent(
        id=row["id"],
        source=row["source"],
        external_id=row["external_id"],
        type=row["type"],
        occurred_at=_utc(row["occurred_at"]),
        ingested_at=_utc(row["ingested_at"]),
        payload=row["payload"],
    )


def _window_conditions(
    start: datetime,
    end: datetime,
    *,
    source: str | None = None,
    event_type: str | None = None,
) -> list[Any]:
    conditions: list[Any] = [
        events.c.occurred_at >= start,
        events.c.occurred_at < end,
    ]
    if source is not None:
        conditions.append(events.c.source == source)
    if event_type is not None:
        conditions.append(events.c.type == event_type)
    return conditions


class EventRepository:
    """Owns a supplied SQLAlchemy session; commits writes, rolls back DB errors."""

    def __init__(
        self,
        session: Session,
        *,
        read_max_retries: int = 0,
        read_retry_backoff_ms: int = 50,
    ) -> None:
        self._session = session
        self._read_max_retries = max(0, read_max_retries)
        self._read_retry_backoff_ms = max(0, read_retry_backoff_ms)

    def _rollback(self) -> None:
        try:
            self._session.rollback()
        except SQLAlchemyError as exc:
            raise StorageUnavailable("Event storage unavailable") from exc

    def _run_read(self, operation: Callable[[], T]) -> T:
        """Execute an idempotent read with bounded exponential backoff on OperationalError."""
        attempts = self._read_max_retries + 1
        for attempt in range(attempts):
            try:
                return operation()
            except OperationalError as exc:
                self._rollback()
                if attempt + 1 < attempts:
                    if self._read_retry_backoff_ms > 0:
                        delay = (self._read_retry_backoff_ms * (2**attempt)) / 1000.0
                        time.sleep(delay)
                    continue
                raise StorageUnavailable("Event storage unavailable") from exc
            except SQLAlchemyError as exc:
                self._rollback()
                raise StorageUnavailable("Event storage unavailable") from exc
        raise StorageUnavailable("Event storage unavailable")

    def create(
        self,
        *,
        source: str,
        external_id: str,
        event_type: str,
        occurred_at: datetime,
        payload: dict,
    ) -> StoredEvent:
        occurred_at = _utc(occurred_at)
        try:
            row = (
                self._session.execute(
                    insert(events)
                    .values(
                        id=uuid4(),
                        source=source,
                        external_id=external_id,
                        type=event_type,
                        occurred_at=occurred_at,
                        ingested_at=datetime.now(UTC),
                        payload=payload,
                    )
                    .returning(*events.c)
                )
                .mappings()
                .one()
            )
            stored = _stored_event(row)
            self._session.commit()
            return stored
        except IntegrityError as exc:
            self._rollback()
            if (
                getattr(exc.orig, "sqlstate", None) == "23505"
                and getattr(getattr(exc.orig, "diag", None), "constraint_name", None)
                == "uq_events_source_external_id"
            ):
                raise DuplicateEvent("Event already exists") from exc
            raise StorageUnavailable("Event storage unavailable") from exc
        except SQLAlchemyError as exc:
            self._rollback()
            raise StorageUnavailable("Event storage unavailable") from exc

    def create_batch(
        self,
        items: Sequence[dict[str, Any]],
    ) -> list[StoredEvent]:
        """Atomically persist a batch of events; rolls back all items on any duplicate."""
        if not items:
            return []
        now = datetime.now(UTC)
        values = [
            {
                "id": uuid4(),
                "source": item["source"],
                "external_id": item["external_id"],
                "type": item["event_type"],
                "occurred_at": _utc(item["occurred_at"]),
                "ingested_at": now,
                "payload": item["payload"],
            }
            for item in items
        ]
        try:
            rows = (
                self._session.execute(
                    insert(events).values(values).returning(*events.c)
                )
                .mappings()
                .all()
            )
            stored = [_stored_event(row) for row in rows]
            self._session.commit()
            return stored
        except IntegrityError as exc:
            self._rollback()
            if (
                getattr(exc.orig, "sqlstate", None) == "23505"
                and getattr(getattr(exc.orig, "diag", None), "constraint_name", None)
                == "uq_events_source_external_id"
            ):
                raise DuplicateEvent("Event already exists") from exc
            raise StorageUnavailable("Event storage unavailable") from exc
        except SQLAlchemyError as exc:
            self._rollback()
            raise StorageUnavailable("Event storage unavailable") from exc

    def get(self, event_id: UUID) -> StoredEvent | None:
        def query() -> StoredEvent | None:
            row = (
                self._session.execute(select(events).where(events.c.id == event_id))
                .mappings()
                .one_or_none()
            )
            return _stored_event(row) if row is not None else None

        return self._run_read(query)

    def list_events(
        self,
        *,
        start: datetime,
        end: datetime,
        source: str | None = None,
        event_type: str | None = None,
        limit: int = 50,
        offset: int = 0,
    ) -> tuple[int, list[StoredEvent]]:
        start, end = _utc(start), _utc(end)
        if limit < 1 or offset < 0:
            raise ValueError("limit must be >= 1 and offset must be >= 0")
        conditions = _window_conditions(
            start, end, source=source, event_type=event_type
        )

        def query() -> tuple[int, list[StoredEvent]]:
            total = int(
                self._session.execute(
                    select(func.count()).select_from(events).where(*conditions)
                ).scalar_one()
            )
            rows = (
                self._session.execute(
                    select(events)
                    .where(*conditions)
                    .order_by(events.c.occurred_at.asc(), events.c.id.asc())
                    .limit(limit)
                    .offset(offset)
                )
                .mappings()
                .all()
            )
            return total, [_stored_event(row) for row in rows]

        return self._run_read(query)

    def counts(
        self, *, start: datetime, end: datetime, group_by: Literal["type", "source"]
    ) -> list[tuple[str, int]]:
        if group_by not in ("type", "source"):
            raise ValueError("group_by must be 'type' or 'source'")
        start, end = _utc(start), _utc(end)
        grouping = events.c[group_by]

        def query() -> list[tuple[str, int]]:
            rows = self._session.execute(
                select(grouping, func.count())
                .where(events.c.occurred_at >= start, events.c.occurred_at < end)
                .group_by(grouping)
                .order_by(grouping)
            ).all()
            return [(key, count) for key, count in rows]

        return self._run_read(query)

    def timeseries(
        self,
        *,
        start: datetime,
        end: datetime,
        bucket: Literal["minute", "hour", "day"],
        source: str | None = None,
        event_type: str | None = None,
    ) -> list[tuple[datetime, int]]:
        if bucket not in ("minute", "hour", "day"):
            raise ValueError("bucket must be 'minute', 'hour' or 'day'")
        start, end = _utc(start), _utc(end)
        conditions = _window_conditions(
            start, end, source=source, event_type=event_type
        )
        bucket_start = func.timezone(
            "UTC",
            func.date_trunc(bucket, func.timezone("UTC", events.c.occurred_at)),
        ).label("bucket_start")

        def query() -> list[tuple[datetime, int]]:
            rows = self._session.execute(
                select(bucket_start, func.count())
                .where(*conditions)
                .group_by(bucket_start)
                .order_by(bucket_start.asc())
            ).all()
            return [(_utc(ts), int(count)) for ts, count in rows]

        return self._run_read(query)

    def summary(
        self,
        *,
        start: datetime,
        end: datetime,
        source: str | None = None,
        event_type: str | None = None,
        numeric_field: str = "value",
    ) -> dict[str, Any]:
        start, end = _utc(start), _utc(end)
        conditions = _window_conditions(
            start, end, source=source, event_type=event_type
        )
        json_field = events.c.payload[numeric_field]
        numeric_value = case(
            (func.jsonb_typeof(json_field) == "number", cast(json_field.astext, Numeric)),
            else_=None,
        )

        def query() -> dict[str, Any]:
            totals_row = (
                self._session.execute(
                    select(
                        func.count().label("total_events"),
                        func.count(func.distinct(events.c.source)).label("unique_sources"),
                        func.count(func.distinct(events.c.type)).label("unique_types"),
                        func.min(events.c.occurred_at).label("first_occurred_at"),
                        func.max(events.c.occurred_at).label("last_occurred_at"),
                        func.count(numeric_value).label("numeric_samples"),
                        func.min(numeric_value).label("numeric_min"),
                        func.max(numeric_value).label("numeric_max"),
                        func.avg(numeric_value).label("numeric_avg"),
                    )
                    .select_from(events)
                    .where(*conditions)
                )
                .mappings()
                .one()
            )
            by_type_rows = self._session.execute(
                select(events.c.type, func.count())
                .where(*conditions)
                .group_by(events.c.type)
                .order_by(events.c.type.asc())
            ).all()
            by_source_rows = self._session.execute(
                select(events.c.source, func.count())
                .where(*conditions)
                .group_by(events.c.source)
                .order_by(events.c.source.asc())
            ).all()

            first_at = totals_row["first_occurred_at"]
            last_at = totals_row["last_occurred_at"]
            samples = int(totals_row["numeric_samples"] or 0)
            return {
                "total_events": int(totals_row["total_events"] or 0),
                "unique_sources": int(totals_row["unique_sources"] or 0),
                "unique_types": int(totals_row["unique_types"] or 0),
                "first_occurred_at": _utc(first_at) if first_at is not None else None,
                "last_occurred_at": _utc(last_at) if last_at is not None else None,
                "by_type": [
                    {"key": key, "count": int(count)} for key, count in by_type_rows
                ],
                "by_source": [
                    {"key": key, "count": int(count)} for key, count in by_source_rows
                ],
                "numeric_summary": {
                    "field": numeric_field,
                    "samples": samples,
                    "min": (
                        float(totals_row["numeric_min"])
                        if samples > 0 and totals_row["numeric_min"] is not None
                        else None
                    ),
                    "max": (
                        float(totals_row["numeric_max"])
                        if samples > 0 and totals_row["numeric_max"] is not None
                        else None
                    ),
                    "avg": (
                        float(totals_row["numeric_avg"])
                        if samples > 0 and totals_row["numeric_avg"] is not None
                        else None
                    ),
                },
            }

        return self._run_read(query)


@lru_cache(maxsize=16)
def _session_factory(
    database_url: str,
    connect_timeout_seconds: int = 3,
    statement_timeout_ms: int = 5000,
) -> sessionmaker[Session]:
    # Cache per URL/timeout: pre_ping replaces stale pooled connections after a DB restart.
    url = make_url(database_url)
    if url.drivername != "postgresql+psycopg":
        raise ArgumentError("Expected PostgreSQL with psycopg")
    engine = create_engine(
        url,
        pool_pre_ping=True,
        connect_args={
            "connect_timeout": connect_timeout_seconds,
            "options": f"-c statement_timeout={statement_timeout_ms}",
        },
    )
    return sessionmaker(bind=engine)


def _resolve_settings(request: Request | None = None) -> Settings:
    if request is not None:
        explicit = getattr(request.app.state, "explicit_settings", None)
        if explicit is not None:
            return explicit
    return Settings.from_env()


def get_repository(
    request: Request = None,  # type: ignore[assignment]
) -> Iterator[EventRepository]:
    """FastAPI dependency: one session per request, configured from DATABASE_URL."""
    settings = _resolve_settings(request)
    if not settings.database_url:
        raise StorageUnavailable("Event storage unavailable")
    try:
        factory = _session_factory(
            settings.database_url,
            settings.db_connect_timeout_seconds,
            settings.db_statement_timeout_ms,
        )
    except (ArgumentError, SQLAlchemyError) as exc:
        raise StorageUnavailable("Event storage unavailable") from exc
    with factory() as session:
        yield EventRepository(
            session,
            read_max_retries=settings.db_read_max_retries,
            read_retry_backoff_ms=settings.db_read_retry_backoff_ms,
        )
