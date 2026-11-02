"""Domain operations and business rules for Escenario 3 event ingestion and analysis."""

from datetime import UTC
from uuid import UUID

from .auth import Principal, ensure_permission, ensure_source_access
from .contracts import DuplicateEvent, EventNotFound
from .schemas import (
    CountsQuery,
    EventBatchCreate,
    EventCreate,
    EventsListQuery,
    SummaryQuery,
    TimeseriesQuery,
)
from .storage import EventRepository, StoredEvent


def create_event(
    event: EventCreate, principal: Principal, repository: EventRepository
) -> StoredEvent:
    """Persist an event; enforces role, source scope and (source, external_id) uniqueness."""
    ensure_permission(principal, "events:create")
    ensure_source_access(principal, event.source)
    return repository.create(
        source=event.source,
        external_id=event.external_id,
        event_type=event.type,
        occurred_at=event.occurred_at.astimezone(UTC),
        payload=event.payload,
    )


def create_events_batch(
    batch: EventBatchCreate, principal: Principal, repository: EventRepository
) -> list[StoredEvent]:
    """Atomically persist a batch of events; rejects intra-batch or stored duplicates."""
    ensure_permission(principal, "events:create")
    for item in batch.events:
        ensure_source_access(principal, item.source)

    seen: set[tuple[str, str]] = set()
    items: list[dict[str, object]] = []
    for item in batch.events:
        natural_key = (item.source, item.external_id)
        if natural_key in seen:
            raise DuplicateEvent("Duplicate (source, external_id) inside batch")
        seen.add(natural_key)
        items.append(
            {
                "source": item.source,
                "external_id": item.external_id,
                "event_type": item.type,
                "occurred_at": item.occurred_at.astimezone(UTC),
                "payload": item.payload,
            }
        )
    return repository.create_batch(items)


def get_event(
    event_id: UUID, principal: Principal, repository: EventRepository
) -> StoredEvent:
    ensure_permission(principal, "events:read")
    event = repository.get(event_id)
    if event is None:
        raise EventNotFound(event_id)
    return event


def list_events(
    query: EventsListQuery, principal: Principal, repository: EventRepository
) -> tuple[int, list[StoredEvent]]:
    ensure_permission(principal, "events:read")
    start = query.from_.astimezone(UTC)
    end = query.to.astimezone(UTC)
    return repository.list_events(
        start=start,
        end=end,
        source=query.source,
        event_type=query.type,
        limit=query.limit,
        offset=query.offset,
    )


def count_events(
    query: CountsQuery, principal: Principal, repository: EventRepository
) -> dict[str, object]:
    ensure_permission(principal, "analysis:read")
    # Use the wire aliases rather than assuming the Python name for `from`.
    window = query.model_dump(by_alias=True)
    start = window["from"].astimezone(UTC)
    end = window["to"].astimezone(UTC)
    group_by = window["group_by"]
    buckets = sorted(
        repository.counts(start=start, end=end, group_by=group_by),
        key=lambda item: item[0],
    )
    return {
        "total": sum(count for _, count in buckets),
        "group_by": group_by,
        "window": {"from": start, "to": end},
        "buckets": [{"key": key, "count": count} for key, count in buckets],
    }


def analyze_timeseries(
    query: TimeseriesQuery, principal: Principal, repository: EventRepository
) -> dict[str, object]:
    ensure_permission(principal, "analysis:read")
    start = query.from_.astimezone(UTC)
    end = query.to.astimezone(UTC)
    points = repository.timeseries(
        start=start,
        end=end,
        bucket=query.bucket,
        source=query.source,
        event_type=query.type,
    )
    return {
        "total": sum(count for _, count in points),
        "bucket": query.bucket,
        "window": {"from": start, "to": end},
        "filters": {"source": query.source, "type": query.type},
        "points": [
            {"bucket_start": ts.astimezone(UTC), "count": count}
            for ts, count in points
        ],
    }


def analyze_summary(
    query: SummaryQuery, principal: Principal, repository: EventRepository
) -> dict[str, object]:
    ensure_permission(principal, "analysis:read")
    start = query.from_.astimezone(UTC)
    end = query.to.astimezone(UTC)
    summary = repository.summary(
        start=start,
        end=end,
        source=query.source,
        event_type=query.type,
        numeric_field=query.numeric_field,
    )
    return {
        "window": {"from": start, "to": end},
        "filters": {"source": query.source, "type": query.type},
        **summary,
    }
