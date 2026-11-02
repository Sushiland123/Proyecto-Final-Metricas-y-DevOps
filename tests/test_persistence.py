"""PostgreSQL integration tests; never run against a non-test database."""

import os
from concurrent.futures import ThreadPoolExecutor
from datetime import UTC, datetime, timedelta, timezone
from pathlib import Path
from threading import Barrier
from types import SimpleNamespace
from unittest.mock import Mock
from uuid import uuid4

import pytest
from alembic.config import Config
from event_service.contracts import DuplicateEvent, StorageUnavailable
from sqlalchemy import create_engine, delete, inspect, select
from sqlalchemy.engine import make_url
from sqlalchemy.exc import IntegrityError, OperationalError
from sqlalchemy.orm import Session

from alembic import command
from event_service.storage import EventRepository, events, get_repository

ROOT = Path(__file__).resolve().parents[1]


@pytest.fixture
def migrated_database(monkeypatch):
    url = os.environ.get("TEST_DATABASE_URL")
    if not url:
        pytest.skip("Set TEST_DATABASE_URL to a dedicated PostgreSQL test database")
    parsed = make_url(url)
    name = parsed.database or ""
    if parsed.drivername != "postgresql+psycopg" or not (
        name.startswith("test_") or name.endswith("_test")
    ):
        pytest.fail("TEST_DATABASE_URL must use psycopg and a test_*/ *_test database")
    monkeypatch.setenv("DATABASE_URL", url)
    monkeypatch.setenv("EVENT_PRODUCER_KEY", "com03-test-producer-key")
    monkeypatch.setenv("EVENT_ANALYST_KEY", "com03-test-analyst-key")
    command.upgrade(Config(str(ROOT / "alembic.ini")), "head")
    return url


@pytest.fixture
def test_source(migrated_database):
    source = f"com03-{uuid4().hex[:16]}"
    yield source
    # Delete only this test's records; never drop a table or a database.
    engine = create_engine(migrated_database)
    try:
        with Session(engine) as session:
            session.execute(delete(events).where(events.c.source == source))
            session.commit()
    finally:
        engine.dispose()


def _create(session: Session, source: str, external_id: str, **overrides):
    params = {
        "source": source,
        "external_id": external_id,
        "event_type": "temperature",
        "occurred_at": datetime(2026, 10, 21, 12, tzinfo=UTC),
        "payload": {"value": 23.5, "nested": {"unit": "C"}},
    }
    params.update(overrides)
    return EventRepository(session).create(**params)


def test_write_read_and_half_open_counts(migrated_database, test_source):
    engine = create_engine(migrated_database)
    try:
        start = datetime(2026, 10, 21, 12, tzinfo=UTC)
        end = datetime(2026, 10, 21, 13, tzinfo=UTC)
        with Session(engine) as session:
            initial_count = dict(
                EventRepository(session).counts(start=start, end=end, group_by="type")
            ).get("temperature", 0)
        with Session(engine) as session:
            original = _create(
                session,
                test_source,
                uuid4().hex,
                occurred_at=datetime(
                    2026, 10, 21, 14, tzinfo=timezone(timedelta(hours=2))
                ),
            )
            _create(
                session,
                test_source,
                uuid4().hex,
                occurred_at=datetime(2026, 10, 21, 13, tzinfo=UTC),
            )
        with Session(engine) as session:
            repo = EventRepository(session)
            assert repo.get(original.id) == original
            assert repo.get(uuid4()) is None
            assert original.occurred_at == datetime(2026, 10, 21, 12, tzinfo=UTC)
            assert original.ingested_at.utcoffset() == timedelta(0)
            assert (
                dict(repo.counts(start=start, end=end, group_by="type"))["temperature"]
                == initial_count + 1
            )
            # The source is unique to this test even if the test DB has other data.
            buckets = repo.counts(
                start=start,
                end=end,
                group_by="source",
            )
            assert (test_source, 1) in buckets
    finally:
        engine.dispose()


def test_concurrent_duplicate_keeps_first_event(migrated_database, test_source):
    engine = create_engine(migrated_database)
    external_id = uuid4().hex
    barrier = Barrier(2)

    def attempt(value):
        with Session(engine) as session:
            barrier.wait(timeout=10)
            try:
                return _create(
                    session,
                    test_source,
                    external_id,
                    payload={"value": value},
                )
            except DuplicateEvent:
                return None

    try:
        with ThreadPoolExecutor(max_workers=2) as executor:
            results = list(executor.map(attempt, (1, 2)))
        successful = [result for result in results if result is not None]
        assert len(successful) == 1
        with Session(engine) as session:
            assert EventRepository(session).get(successful[0].id) == successful[0]
            assert (
                session.scalar(
                    select(events.c.id).where(
                        events.c.source == test_source,
                        events.c.external_id == external_id,
                    )
                )
                == successful[0].id
            )
    finally:
        engine.dispose()


def test_fastapi_dependency_opens_independent_sessions(migrated_database, test_source):
    first = get_repository()
    try:
        original = next(first).create(
            source=test_source,
            external_id=uuid4().hex,
            event_type="temperature",
            occurred_at=datetime(2026, 10, 21, 12, tzinfo=UTC),
            payload={"value": 23.5},
        )
    finally:
        first.close()

    second = get_repository()
    try:
        assert next(second).get(original.id) == original
    finally:
        second.close()


def test_data_survives_new_engine_and_repeated_upgrade(migrated_database, test_source):
    config = Config(str(ROOT / "alembic.ini"))
    engine = create_engine(migrated_database)
    try:
        with Session(engine) as session:
            original = _create(session, test_source, uuid4().hex)
    finally:
        engine.dispose()  # Simulate the API process releasing every connection.

    # Alembic uses alembic_version, so upgrading head again must not rewrite data.
    command.upgrade(config, "head")
    command.upgrade(config, "head")
    new_engine = create_engine(migrated_database)
    try:
        with Session(new_engine) as session:
            assert EventRepository(session).get(original.id) == original
            assert session.scalar(select(events.c.id).where(events.c.id == original.id))
        with new_engine.connect() as connection:
            assert (
                connection.exec_driver_sql(
                    "SELECT version_num FROM alembic_version"
                ).scalar_one()
                == "20261102_02"
            )
            inspector = inspect(connection)
            columns = {
                column["name"]: column for column in inspector.get_columns("events")
            }
            assert str(columns["payload"]["type"]) == "JSONB"
            assert columns["occurred_at"]["type"].timezone
            assert columns["ingested_at"]["type"].timezone
            assert any(
                {"source", "external_id"} == set(constraint["column_names"])
                for constraint in inspector.get_unique_constraints("events")
            )
            assert any(
                index["name"] == "ix_events_occurred_at_type"
                and index["column_names"] == ["occurred_at", "type"]
                for index in inspector.get_indexes("events")
            )
            assert any(
                index["name"] == "ix_events_occurred_at_source"
                and index["column_names"] == ["occurred_at", "source"]
                for index in inspector.get_indexes("events")
            )
    finally:
        new_engine.dispose()


def test_db_failure_rolls_back_and_has_safe_error():
    session = Mock(spec=Session)
    session.execute.side_effect = OperationalError("SELECT", {}, Exception("offline"))
    with pytest.raises(StorageUnavailable, match="Event storage unavailable"):
        EventRepository(session).get(uuid4())
    session.rollback.assert_called_once()


def test_other_integrity_error_is_not_reported_as_duplicate():
    session = Mock(spec=Session)
    original_error = Exception("some other constraint")
    original_error.sqlstate = "23505"
    original_error.diag = SimpleNamespace(constraint_name="events_pkey")
    session.execute.side_effect = IntegrityError("INSERT", {}, original_error)
    with pytest.raises(StorageUnavailable, match="Event storage unavailable"):
        _create(session, "test", "test")
    session.rollback.assert_called_once()


def test_naive_time_is_not_silently_treated_as_utc():
    session = Mock(spec=Session)
    with pytest.raises(ValueError, match="timezone"):
        _create(session, "test", "test", occurred_at=datetime(2026, 10, 21))
    session.execute.assert_not_called()


def test_create_batch_atomicity_and_duplicate_handling(
    migrated_database, test_source
):
    engine = create_engine(migrated_database)
    try:
        with Session(engine) as session:
            repo = EventRepository(session)
            batch = [
                {
                    "source": test_source,
                    "external_id": f"ext-{i}",
                    "event_type": "temperature",
                    "occurred_at": datetime(2026, 10, 21, 12, i, tzinfo=UTC),
                    "payload": {"value": 20.0 + i},
                }
                for i in range(3)
            ]
            saved = repo.create_batch(batch)
            assert len(saved) == 3

        # A batch containing a duplicate fails atomically (none added)
        with Session(engine) as session:
            repo = EventRepository(session)
            duplicate_batch = [
                {
                    "source": test_source,
                    "external_id": "ext-99",
                    "event_type": "temperature",
                    "occurred_at": datetime(2026, 10, 21, 12, 50, tzinfo=UTC),
                    "payload": {"value": 99.0},
                },
                {
                    "source": test_source,
                    "external_id": "ext-0",  # collision with existing ext-0
                    "event_type": "temperature",
                    "occurred_at": datetime(2026, 10, 21, 12, 51, tzinfo=UTC),
                    "payload": {"value": 0.0},
                },
            ]
            with pytest.raises(DuplicateEvent):
                repo.create_batch(duplicate_batch)

        # Confirm ext-99 was rolled back and not persisted
        with Session(engine) as session:
            ext_99 = session.scalar(
                select(events.c.id).where(
                    events.c.source == test_source,
                    events.c.external_id == "ext-99",
                )
            )
            assert ext_99 is None
    finally:
        engine.dispose()


def test_list_events_pagination_and_filters(migrated_database, test_source):
    engine = create_engine(migrated_database)
    try:
        start = datetime(2026, 10, 21, 10, tzinfo=UTC)
        end = datetime(2026, 10, 21, 14, tzinfo=UTC)
        with Session(engine) as session:
            repo = EventRepository(session)
            items = [
                {
                    "source": test_source,
                    "external_id": f"p-{i}",
                    "event_type": "pressure" if i % 2 == 0 else "temp",
                    "occurred_at": datetime(2026, 10, 21, 11, i, tzinfo=UTC),
                    "payload": {"reading": i},
                }
                for i in range(5)
            ]
            repo.create_batch(items)

        with Session(engine) as session:
            repo = EventRepository(session)
            total, page1 = repo.list_events(
                start=start,
                end=end,
                source=test_source,
                limit=2,
                offset=0,
            )
            assert total == 5
            assert len(page1) == 2
            assert page1[0].external_id == "p-0"
            assert page1[1].external_id == "p-1"

            # Filter by event_type
            _, filtered = repo.list_events(
                start=start,
                end=end,
                source=test_source,
                event_type="pressure",
                limit=10,
                offset=0,
            )
            assert len(filtered) == 3
            assert all(ev.type == "pressure" for ev in filtered)
    finally:
        engine.dispose()


def test_timeseries_and_summary_analytics(migrated_database, test_source):
    engine = create_engine(migrated_database)
    try:
        start = datetime(2026, 10, 21, 0, tzinfo=UTC)
        end = datetime(2026, 10, 22, 0, tzinfo=UTC)
        with Session(engine) as session:
            repo = EventRepository(session)
            items = [
                {
                    "source": test_source,
                    "external_id": "ts-1",
                    "event_type": "metric",
                    "occurred_at": datetime(2026, 10, 21, 10, 15, tzinfo=UTC),
                    "payload": {"val": 10.5},
                },
                {
                    "source": test_source,
                    "external_id": "ts-2",
                    "event_type": "metric",
                    "occurred_at": datetime(2026, 10, 21, 10, 45, tzinfo=UTC),
                    "payload": {"val": 20.5},
                },
                {
                    "source": test_source,
                    "external_id": "ts-3",
                    "event_type": "metric",
                    "occurred_at": datetime(2026, 10, 21, 11, 5, tzinfo=UTC),
                    "payload": {"val": 30.0},
                },
            ]
            repo.create_batch(items)

        with Session(engine) as session:
            repo = EventRepository(session)
            ts = repo.timeseries(
                start=start,
                end=end,
                bucket="hour",
                source=test_source,
            )
            assert len(ts) == 2
            assert ts[0] == (datetime(2026, 10, 21, 10, tzinfo=UTC), 2)
            assert ts[1] == (datetime(2026, 10, 21, 11, tzinfo=UTC), 1)

            summary = repo.summary(
                start=start,
                end=end,
                source=test_source,
                numeric_field="val",
            )
            assert summary["total_events"] == 3
            assert summary["unique_sources"] == 1
            assert summary["unique_types"] == 1
            assert summary["numeric_summary"]["samples"] == 3
            assert summary["numeric_summary"]["min"] == 10.5
            assert summary["numeric_summary"]["max"] == 30.0
            assert summary["numeric_summary"]["avg"] == (10.5 + 20.5 + 30.0) / 3
    finally:
        engine.dispose()

