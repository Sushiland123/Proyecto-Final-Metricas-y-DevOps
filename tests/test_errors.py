"""COM-04 boundary validation and uniform, non-disclosing HTTP errors."""

import json
from datetime import datetime, timezone
from importlib.util import find_spec
from typing import Annotated
from uuid import UUID

import pytest
from pydantic import ValidationError

from event_service.schemas import (
    CountsQuery,
    EventBatchCreate,
    EventCreate,
    EventsListQuery,
    SummaryQuery,
    TimeseriesQuery,
)

VALID_EVENT = {
    "source": "sensor-a",
    "type": "temperature",
    "external_id": "e-001",
    "occurred_at": "2026-10-21T12:00:00+02:00",
    "payload": {"value": 23.5, "tags": ["a", None, True]},
}
VALID_QUERY = {
    "from": "2026-10-21T12:00:00+02:00",
    "to": "2026-10-21T13:00:00+02:00",
    "group_by": "type",
}


def test_event_accepts_json_object_and_normalizes_utc():
    event = EventCreate.model_validate(VALID_EVENT)
    assert event.occurred_at == datetime(2026, 10, 21, 10, tzinfo=timezone.utc)
    assert event.model_dump(mode="json")["occurred_at"] == "2026-10-21T10:00:00Z"
    assert event.payload == VALID_EVENT["payload"]


@pytest.mark.parametrize("field", ["source", "type"])
@pytest.mark.parametrize("invalid", ["", "a" * 65, "a b", "../x", "é", "a\n"])
def test_event_rejects_invalid_identifiers(field, invalid):
    with pytest.raises(ValidationError):
        EventCreate.model_validate({**VALID_EVENT, field: invalid})


@pytest.mark.parametrize("invalid", ["", "x" * 129])
def test_event_rejects_invalid_external_ids(invalid):
    with pytest.raises(ValidationError):
        EventCreate.model_validate({**VALID_EVENT, "external_id": invalid})


@pytest.mark.parametrize("invalid", ["2026-10-21T12:00:00", 1792584000])
def test_event_requires_datetime_with_zone_not_timestamp(invalid):
    with pytest.raises(ValidationError):
        EventCreate.model_validate({**VALID_EVENT, "occurred_at": invalid})


def test_event_rejects_extra_fields():
    with pytest.raises(ValidationError):
        EventCreate.model_validate({**VALID_EVENT, "api_key": "must-not-appear"})


def test_payload_accepts_16_kib_and_rejects_one_byte_more():
    overhead = len(json.dumps({"data": ""}, separators=(",", ":")).encode("utf-8"))
    limit = "x" * (16 * 1024 - overhead)
    assert EventCreate.model_validate(
        {**VALID_EVENT, "payload": {"data": limit}}
    ).payload == {"data": limit}
    with pytest.raises(ValidationError):
        EventCreate.model_validate({**VALID_EVENT, "payload": {"data": limit + "x"}})


@pytest.mark.parametrize(
    "invalid", [[], "hello", {"x": float("nan")}, {"x": object()}, {1: "value"}]
)
def test_payload_must_be_json_object(invalid):
    with pytest.raises(ValidationError):
        EventCreate.model_validate({**VALID_EVENT, "payload": invalid})


def test_counts_query_uses_from_alias_and_normalizes_utc():
    query = CountsQuery.model_validate(VALID_QUERY)
    assert query.from_ == datetime(2026, 10, 21, 10, tzinfo=timezone.utc)
    assert query.to == datetime(2026, 10, 21, 11, tzinfo=timezone.utc)
    assert (
        query.model_dump(by_alias=True, mode="json")["from"] == "2026-10-21T10:00:00Z"
    )
    assert (
        CountsQuery.model_validate({**VALID_QUERY, "group_by": "source"}).group_by
        == "source"
    )


@pytest.mark.parametrize(
    "changes",
    [
        {"to": "2026-10-21T10:00:00Z"},
        {"to": "2026-10-21T09:59:59Z"},
        {"from": "2026-10-21T12:00:00"},
        {"to": 1792584000},
        {"group_by": "payload"},
        {"unexpected": "no"},
    ],
)
def test_counts_query_rejects_invalid_windows_or_groupings(changes):
    with pytest.raises(ValidationError):
        CountsQuery.model_validate({**VALID_QUERY, **changes})


def test_counts_query_requires_from_alias_not_python_field_name():
    with pytest.raises(ValidationError):
        CountsQuery.model_validate({**VALID_QUERY, "from_": VALID_QUERY["from"]})


def test_batch_schema_validation():
    # Valid batch with 1 event
    valid = EventBatchCreate.model_validate({"events": [VALID_EVENT]})
    assert len(valid.events) == 1

    # Rejects empty batch
    with pytest.raises(ValidationError):
        EventBatchCreate.model_validate({"events": []})

    # Rejects batch exceeding 100 events
    with pytest.raises(ValidationError):
        EventBatchCreate.model_validate({"events": [VALID_EVENT] * 101})


def test_events_list_query_validation():
    window = {"from": VALID_QUERY["from"], "to": VALID_QUERY["to"]}
    valid = EventsListQuery.model_validate(
        {
            **window,
            "source": "sensor-a",
            "type": "temp",
            "limit": 25,
            "offset": 50,
        }
    )
    assert valid.limit == 25
    assert valid.offset == 50
    assert valid.source == "sensor-a"

    # Rejects booleans for limit / offset
    with pytest.raises(ValidationError):
        EventsListQuery.model_validate({**window, "limit": True})
    with pytest.raises(ValidationError):
        EventsListQuery.model_validate({**window, "offset": False})

    # Rejects limit out of bounds
    with pytest.raises(ValidationError):
        EventsListQuery.model_validate({**window, "limit": 0})
    with pytest.raises(ValidationError):
        EventsListQuery.model_validate({**window, "limit": 101})

    # Rejects negative offset
    with pytest.raises(ValidationError):
        EventsListQuery.model_validate({**window, "offset": -1})


def test_timeseries_query_validation():
    window = {"from": VALID_QUERY["from"], "to": VALID_QUERY["to"]}
    valid = TimeseriesQuery.model_validate({**window, "bucket": "hour"})
    assert valid.bucket == "hour"

    # Rejects invalid bucket
    with pytest.raises(ValidationError):
        TimeseriesQuery.model_validate({**window, "bucket": "month"})

    # Minute bucket span cannot exceed 7 days
    with pytest.raises(ValidationError):
        TimeseriesQuery.model_validate(
            {
                "from": "2026-01-01T00:00:00Z",
                "to": "2026-01-08T00:00:01Z",
                "bucket": "minute",
            }
        )

    # Hour bucket span cannot exceed 90 days
    with pytest.raises(ValidationError):
        TimeseriesQuery.model_validate(
            {
                "from": "2026-01-01T00:00:00Z",
                "to": "2026-04-02T00:00:01Z",
                "bucket": "hour",
            }
        )


def test_summary_query_validation():
    window = {"from": VALID_QUERY["from"], "to": VALID_QUERY["to"]}
    valid = SummaryQuery.model_validate(
        {**window, "numeric_field": "temperature_c"}
    )
    assert valid.numeric_field == "temperature_c"

    # Rejects numeric field with special characters / SQL injection attempt
    with pytest.raises(ValidationError):
        SummaryQuery.model_validate({**window, "numeric_field": "val; DROP TABLE"})



@pytest.fixture
def http_app():
    # COM-02 owns contracts.py. This gate skips only integration tests while
    # developing on the independent base branch; schema tests always run.
    if find_spec("event_service.contracts") is None:
        pytest.skip("COM-02 contracts.py has not been integrated yet")

    from event_service.contracts import (
        DuplicateEvent,
        EventNotFound,
        StorageUnavailable,
    )
    from fastapi import FastAPI, Query
    from starlette.exceptions import HTTPException

    from event_service.errors import register_handlers

    app = FastAPI()
    register_handlers(app)
    register_handlers(app)  # Registering twice must not duplicate middleware.
    availability = {"up": False}

    @app.post("/events")
    def create_event(event: EventCreate):
        return event.model_dump(mode="json")

    @app.get("/counts")
    def counts(query: Annotated[CountsQuery, Query()]):
        return query.model_dump(by_alias=True, mode="json")

    @app.get("/failure/{kind}")
    def failure(kind: str):
        if kind == "duplicate":
            raise DuplicateEvent("secret-event-payload")
        if kind == "missing":
            raise EventNotFound("secret-event-id")
        if kind == "dependency":
            raise StorageUnavailable("secret-database-url")
        if kind == "authentication":
            raise HTTPException(401, detail="secret-api-key")
        if kind == "authorization":
            raise HTTPException(403, detail="secret-identity")
        if kind == "unexpected":
            raise RuntimeError("secret-internal-stack")
        raise HTTPException(404, detail="secret-unknown-path")

    @app.get("/probe")
    def probe():
        if not availability["up"]:
            raise StorageUnavailable("secret-database-url")
        return {"ready": True}

    return app, availability


@pytest.fixture
def client(http_app):
    from fastapi.testclient import TestClient

    with TestClient(http_app[0], raise_server_exceptions=False) as test_client:
        yield test_client


def assert_public_error(response, status, code):
    messages = {
        "INVALID_INPUT": "Invalid request input.",
        "NOT_FOUND": "Resource not found.",
        "DUPLICATE": "Event already exists.",
        "DEPENDENCY_UNAVAILABLE": "Dependency unavailable.",
        "UNAUTHENTICATED": "Authentication required.",
        "FORBIDDEN": "Access denied.",
        "INTERNAL_ERROR": "Internal server error.",
    }
    assert response.status_code == status
    assert response.json() == {
        "error": {
            "code": code,
            "message": messages[code],
            "request_id": response.headers["x-request-id"],
        }
    }


def test_http_valid_event_and_query(client):
    result = client.post(
        "/events", json=VALID_EVENT, headers={"X-Request-ID": "req-123"}
    )
    assert result.status_code == 200
    assert result.headers["x-request-id"] == "req-123"
    assert result.json()["occurred_at"] == "2026-10-21T10:00:00Z"
    result = client.get("/counts", params=VALID_QUERY)
    assert result.status_code == 200
    assert result.json()["from"] == "2026-10-21T10:00:00Z"


@pytest.mark.parametrize(
    "method,path,kwargs",
    [
        ("POST", "/events", {"json": {**VALID_EVENT, "payload": ["secret-input"]}}),
        ("POST", "/events", {"content": b'{"payload":"secret-input",'}),
        ("GET", "/counts", {"params": {**VALID_QUERY, "group_by": "secret-input"}}),
    ],
)
def test_http_invalid_input_hides_values_and_logs_are_safe(
    client, caplog, method, path, kwargs
):
    response = client.request(method, path, **kwargs)
    assert_public_error(response, 422, "INVALID_INPUT")
    assert "secret-input" not in response.text + caplog.text
    assert response.headers["x-request-id"] in caplog.text


@pytest.mark.parametrize(
    "path,status,code",
    [
        ("/failure/duplicate", 409, "DUPLICATE"),
        ("/failure/missing", 404, "NOT_FOUND"),
        ("/failure/dependency", 503, "DEPENDENCY_UNAVAILABLE"),
        ("/failure/authentication", 401, "UNAUTHENTICATED"),
        ("/failure/authorization", 403, "FORBIDDEN"),
        ("/failure/unexpected", 500, "INTERNAL_ERROR"),
        ("/failure/not-a-route", 404, "NOT_FOUND"),
        ("/missing-http-route", 404, "NOT_FOUND"),
    ],
)
def test_http_expected_and_unexpected_errors_are_uniform_and_safe(
    client, caplog, path, status, code
):
    response = client.get(path, headers={"X-Request-ID": "request-1"})
    assert_public_error(response, status, code)
    assert response.headers["x-request-id"] == "request-1"
    assert f"code={code} status={status} request_id=request-1" in caplog.text
    assert "secret-" not in response.text + caplog.text
    assert "Traceback" not in response.text + caplog.text


def test_logs_route_template_not_client_supplied_path_value(client, caplog):
    response = client.get("/failure/secret-in-url")
    assert_public_error(response, 404, "NOT_FOUND")
    assert "route=/failure/{kind}" in caplog.text
    assert "secret-in-url" not in caplog.text


@pytest.mark.parametrize(
    "headers",
    [
        {"X-Request-ID": "contains spaces and secret?"},
        {"X-Request-ID": "x" * 129},
        [("X-Request-ID", "first"), ("X-Request-ID", "second")],
    ],
)
def test_invalid_or_duplicate_request_id_replaced_not_reflected(client, headers):
    response = client.get("/failure/authentication", headers=headers)
    assert_public_error(response, 401, "UNAUTHENTICATED")
    UUID(response.headers["x-request-id"])
    assert "secret?" not in response.text


def test_dependency_recovery_without_recreating_app_or_client(client, http_app):
    unavailable = client.get("/probe", headers={"X-Request-ID": "down-1"})
    assert_public_error(unavailable, 503, "DEPENDENCY_UNAVAILABLE")

    http_app[1]["up"] = True
    recovered = client.get("/probe", headers={"X-Request-ID": "up-2"})
    assert recovered.status_code == 200
    assert recovered.json() == {"ready": True}
    assert recovered.headers["x-request-id"] == "up-2"
