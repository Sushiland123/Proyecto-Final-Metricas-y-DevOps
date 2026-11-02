"""Access controls exercised independently of the yet-to-be-integrated app."""

import logging
from typing import Annotated

import pytest
from fastapi import Depends, FastAPI, HTTPException
from fastapi.testclient import TestClient

from event_service.auth import (
    Principal,
    ensure_permission,
    ensure_source_access,
    require_analyst,
    require_producer,
)


@pytest.fixture(autouse=True)
def isolated_keys(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.delenv("EVENT_PRODUCER_KEY", raising=False)
    monkeypatch.delenv("EVENT_ANALYST_KEY", raising=False)
    monkeypatch.delenv("EVENT_PRODUCER_SOURCE_KEYS", raising=False)


@pytest.fixture
def configured_keys(monkeypatch: pytest.MonkeyPatch) -> tuple[str, str]:
    producer, analyst = "test-producer-credential", "test-analyst-credential"
    monkeypatch.setenv("EVENT_PRODUCER_KEY", producer)
    monkeypatch.setenv("EVENT_ANALYST_KEY", analyst)
    return producer, analyst


@pytest.fixture
def client() -> TestClient:
    app = FastAPI()

    @app.post("/v1/events")
    def create_event(
        principal: Annotated[Principal, Depends(require_producer)],
    ) -> dict:
        ensure_permission(principal, "events:create")
        return {"role": principal.role}

    @app.get("/v1/events/example")
    def read_event(principal: Annotated[Principal, Depends(require_analyst)]) -> dict:
        ensure_permission(principal, "events:read")
        return {"role": principal.role}

    @app.get("/v1/analysis/counts")
    def read_analysis(
        principal: Annotated[Principal, Depends(require_analyst)],
    ) -> dict:
        ensure_permission(principal, "analysis:read")
        return {"role": principal.role}

    return TestClient(app)


@pytest.mark.parametrize(
    ("method", "path", "role"),
    [
        ("post", "/v1/events", "producer"),
        ("get", "/v1/events/example", "analyst"),
        ("get", "/v1/analysis/counts", "analyst"),
    ],
)
def test_authorized_route(
    client: TestClient,
    configured_keys: tuple[str, str],
    method: str,
    path: str,
    role: str,
) -> None:
    key = configured_keys[0] if role == "producer" else configured_keys[1]
    response = getattr(client, method)(path, headers={"x-api-key": key})
    assert response.status_code == 200
    assert response.json() == {"role": role}


@pytest.mark.parametrize(
    ("method", "path", "key_index"),
    [
        ("post", "/v1/events", 1),
        ("get", "/v1/events/example", 0),
        ("get", "/v1/analysis/counts", 0),
    ],
)
def test_valid_key_cannot_escalate_to_other_role(
    client: TestClient,
    configured_keys: tuple[str, str],
    method: str,
    path: str,
    key_index: int,
) -> None:
    response = getattr(client, method)(
        path, headers={"X-API-Key": configured_keys[key_index]}
    )
    assert response.status_code == 403


@pytest.mark.parametrize("key", [None, "", " ", "unknown", "test-analyst-credential-x"])
@pytest.mark.parametrize(
    "method,path",
    [
        ("post", "/v1/events"),
        ("get", "/v1/events/example"),
        ("get", "/v1/analysis/counts"),
    ],
)
def test_missing_or_incorrect_key_returns_401(
    client: TestClient,
    configured_keys: tuple[str, str],
    method: str,
    path: str,
    key: str | None,
) -> None:
    headers = {} if key is None else {"X-API-Key": key}
    response = getattr(client, method)(path, headers=headers)
    assert response.status_code == 401


@pytest.mark.parametrize(
    ("producer", "analyst"),
    [
        (None, None),
        ("producer-only", None),
        (None, "analyst-only"),
        ("", "analyst-only"),
        ("producer-only", " "),
        ("same-key", "same-key"),
    ],
)
def test_incomplete_or_ambiguous_configuration_fails_closed(
    client: TestClient,
    monkeypatch: pytest.MonkeyPatch,
    producer: str | None,
    analyst: str | None,
) -> None:
    if producer is not None:
        monkeypatch.setenv("EVENT_PRODUCER_KEY", producer)
    if analyst is not None:
        monkeypatch.setenv("EVENT_ANALYST_KEY", analyst)

    for method, path, key in [
        ("post", "/v1/events", producer),
        ("get", "/v1/events/example", analyst),
    ]:
        response = getattr(client, method)(path, headers={"X-API-Key": key or "any"})
        assert response.status_code == 401


@pytest.mark.parametrize(
    ("role", "action", "allowed"),
    [
        ("producer", "events:create", True),
        ("producer", "events:read", False),
        ("producer", "analysis:read", False),
        ("analyst", "events:create", False),
        ("analyst", "events:read", True),
        ("analyst", "analysis:read", True),
    ],
)
def test_service_permission_even_without_route_guard(
    role: str, action: str, allowed: bool
) -> None:
    principal = Principal(role=role)
    if allowed:
        assert ensure_permission(principal, action) is None
    else:
        with pytest.raises(HTTPException) as exc_info:
            ensure_permission(principal, action)
        assert exc_info.value.status_code == 403


def test_credentials_never_appear_in_responses_or_logs(
    client: TestClient,
    configured_keys: tuple[str, str],
    caplog: pytest.LogCaptureFixture,
) -> None:
    producer, analyst = configured_keys
    with caplog.at_level(logging.DEBUG):
        responses = [
            client.post("/v1/events", headers={"X-API-Key": producer}),
            client.post("/v1/events", headers={"X-API-Key": analyst}),
            client.get("/v1/events/example", headers={"X-API-Key": producer + "x"}),
        ]
    assert [response.status_code for response in responses] == [200, 403, 401]
    for secret in configured_keys:
        assert secret not in caplog.text
        for response in responses:
            assert secret not in response.text


def test_source_scoped_producer_authorization(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("EVENT_PRODUCER_KEY", "global-producer-key")
    monkeypatch.setenv("EVENT_ANALYST_KEY", "analyst-key")
    monkeypatch.setenv(
        "EVENT_PRODUCER_SOURCE_KEYS", "sensor-a:scoped-key-a,sensor-b:scoped-key-b"
    )

    app = FastAPI()

    @app.post("/v1/events/{source}")
    def create_event(
        source: str,
        principal: Annotated[Principal, Depends(require_producer)],
    ) -> dict:
        ensure_source_access(principal, source)
        return {"role": principal.role, "source": source}

    client = TestClient(app)

    # Scoped producer can publish to authorized source
    ok_a = client.post("/v1/events/sensor-a", headers={"X-API-Key": "scoped-key-a"})
    assert ok_a.status_code == 200
    assert ok_a.json() == {"role": "producer", "source": "sensor-a"}

    # Scoped producer cannot publish to another source (lateral isolation)
    forbidden = client.post(
        "/v1/events/sensor-b", headers={"X-API-Key": "scoped-key-a"}
    )
    assert forbidden.status_code == 403

    # Global producer can publish to any source
    global_a = client.post(
        "/v1/events/sensor-a", headers={"X-API-Key": "global-producer-key"}
    )
    assert global_a.status_code == 200
    global_b = client.post(
        "/v1/events/sensor-b", headers={"X-API-Key": "global-producer-key"}
    )
    assert global_b.status_code == 200

    # Analyst cannot publish even with analyst key
    analyst_try = client.post(
        "/v1/events/sensor-a", headers={"X-API-Key": "analyst-key"}
    )
    assert analyst_try.status_code == 403


@pytest.mark.parametrize(
    "bad_source_keys",
    [
        "invalid source format:key1",
        "sensor-a",
        "sensor-a:key1,sensor-a:key2",
        "sensor-a:global-producer-key",
        "sensor-a:analyst-key",
    ],
)
def test_malformed_or_colliding_source_keys_fail_closed(
    monkeypatch: pytest.MonkeyPatch, bad_source_keys: str
) -> None:
    monkeypatch.setenv("EVENT_PRODUCER_KEY", "global-producer-key")
    monkeypatch.setenv("EVENT_ANALYST_KEY", "analyst-key")
    monkeypatch.setenv("EVENT_PRODUCER_SOURCE_KEYS", bad_source_keys)

    app = FastAPI()

    @app.post("/v1/events/{source}")
    def create_event(
        source: str,
        principal: Annotated[Principal, Depends(require_producer)],
    ) -> dict:
        ensure_source_access(principal, source)
        return {"ok": True}

    client = TestClient(app)
    response = client.post(
        "/v1/events/sensor-a", headers={"X-API-Key": "global-producer-key"}
    )
    assert response.status_code == 401

