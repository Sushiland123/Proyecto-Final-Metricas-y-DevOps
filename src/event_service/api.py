"""HTTP routes for Escenario 3 ingestion, event exploration and analytical queries."""

from datetime import UTC
from typing import Annotated, Any
from uuid import UUID

from fastapi import APIRouter, Depends, Query

from .auth import Principal, require_analyst, require_producer
from .schemas import (
    CountsQuery,
    EventBatchCreate,
    EventCreate,
    EventsListQuery,
    SummaryQuery,
    TimeseriesQuery,
)
from .service import (
    analyze_summary,
    analyze_timeseries,
    count_events,
    create_event,
    create_events_batch,
    get_event,
    list_events,
)
from .storage import EventRepository, StoredEvent, get_repository

router = APIRouter(prefix="/v1")


def _event_document(event: StoredEvent) -> dict[str, Any]:
    """Only expose fields in the public contract, never ORM internals."""
    return {
        "id": event.id,
        "source": event.source,
        "external_id": event.external_id,
        "type": event.type,
        "occurred_at": event.occurred_at.astimezone(UTC),
        "payload": event.payload,
        "ingested_at": event.ingested_at.astimezone(UTC),
    }


@router.post("/events", status_code=201)
def post_event(
    event: EventCreate,
    principal: Annotated[Principal, Depends(require_producer)],
    repository: Annotated[EventRepository, Depends(get_repository)],
) -> dict[str, Any]:
    return _event_document(create_event(event, principal, repository))


@router.post("/events/batch", status_code=201)
def post_events_batch(
    batch: EventBatchCreate,
    principal: Annotated[Principal, Depends(require_producer)],
    repository: Annotated[EventRepository, Depends(get_repository)],
) -> dict[str, Any]:
    created = create_events_batch(batch, principal, repository)
    return {
        "inserted": len(created),
        "events": [_event_document(event) for event in created],
    }


@router.get("/events")
def read_events(
    query: Annotated[EventsListQuery, Query()],
    principal: Annotated[Principal, Depends(require_analyst)],
    repository: Annotated[EventRepository, Depends(get_repository)],
) -> dict[str, Any]:
    total, items = list_events(query, principal, repository)
    return {
        "total": total,
        "limit": query.limit,
        "offset": query.offset,
        "window": {
            "from": query.from_.astimezone(UTC),
            "to": query.to.astimezone(UTC),
        },
        "filters": {"source": query.source, "type": query.type},
        "items": [_event_document(event) for event in items],
    }


@router.get("/events/{event_id}")
def read_event(
    event_id: UUID,
    principal: Annotated[Principal, Depends(require_analyst)],
    repository: Annotated[EventRepository, Depends(get_repository)],
) -> dict[str, Any]:
    return _event_document(get_event(event_id, principal, repository))


@router.get("/analysis/counts")
def read_counts(
    query: Annotated[CountsQuery, Query()],
    principal: Annotated[Principal, Depends(require_analyst)],
    repository: Annotated[EventRepository, Depends(get_repository)],
) -> dict[str, object]:
    return count_events(query, principal, repository)


@router.get("/analysis/timeseries")
def read_timeseries(
    query: Annotated[TimeseriesQuery, Query()],
    principal: Annotated[Principal, Depends(require_analyst)],
    repository: Annotated[EventRepository, Depends(get_repository)],
) -> dict[str, object]:
    return analyze_timeseries(query, principal, repository)


@router.get("/analysis/summary")
def read_summary(
    query: Annotated[SummaryQuery, Query()],
    principal: Annotated[Principal, Depends(require_analyst)],
    repository: Annotated[EventRepository, Depends(get_repository)],
) -> dict[str, object]:
    return analyze_summary(query, principal, repository)
