"""Defence/CI check: restart API and PostgreSQL, retain the test volume.

Run with ``uv run python scripts/verify_durability.py`` after setting the
environment variables listed in docs/arquitectura/persistencia.md.
"""

import json
import os
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import UTC, datetime
from pathlib import Path
from uuid import uuid4

ROOT = Path(__file__).resolve().parents[1]
APP_PROCESS: subprocess.Popen | None = None
APP_LOG = None


def configure() -> tuple[str, str]:
    for key in (
        "POSTGRES_PASSWORD",
        "DATABASE_URL",
        "EVENT_PRODUCER_KEY",
        "EVENT_ANALYST_KEY",
    ):
        if not os.environ.get(key):
            raise SystemExit(f"{key} is required")
    os.environ.setdefault("POSTGRES_DB", "events_test")
    os.environ.setdefault("POSTGRES_PORT", "15433")
    os.environ.setdefault("COMPOSE_PROJECT_NAME", f"com03-defense-{os.getuid()}")
    if not os.environ["POSTGRES_PORT"].isdecimal():
        raise SystemExit("POSTGRES_PORT must be a valid TCP port")
    url = urllib.parse.urlsplit(os.environ["DATABASE_URL"])
    db_name = url.path.lstrip("/")
    if (
        url.scheme != "postgresql+psycopg"
        or url.hostname not in ("127.0.0.1", "localhost")
        or url.port != int(os.environ["POSTGRES_PORT"])
        or db_name != os.environ["POSTGRES_DB"]
        or not (db_name.startswith("test_") or db_name.endswith("_test"))
    ):
        raise SystemExit(
            "DATABASE_URL must match the local, dedicated *_test Compose DB"
        )
    port = os.environ.get("API_PORT", "18080")
    if not port.isdecimal() or not 1 <= int(port) <= 65535:
        raise SystemExit("API_PORT must be a valid TCP port")
    return os.environ["COMPOSE_PROJECT_NAME"], port


def compose(
    project: str, *args: str, quiet: bool = False
) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["docker", "compose", "-p", project, *args],
        cwd=ROOT,
        check=not quiet,
        stdout=subprocess.DEVNULL if quiet else None,
        stderr=subprocess.DEVNULL if quiet else None,
    )


def wait_postgres(project: str) -> None:
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        result = compose(
            project,
            "exec",
            "-T",
            "postgres",
            "pg_isready",
            "-U",
            os.environ.get("POSTGRES_USER", "events"),
            "-d",
            os.environ["POSTGRES_DB"],
            quiet=True,
        )
        if result.returncode == 0:
            return
        time.sleep(1)
    raise RuntimeError("Test PostgreSQL did not become ready")


def upgrade() -> None:
    subprocess.run(
        [
            sys.executable,
            "-m",
            "alembic",
            "-c",
            str(ROOT / "alembic.ini"),
            "upgrade",
            "head",
        ],
        cwd=ROOT,
        check=True,
    )


def request(
    method: str,
    url: str,
    *,
    key: str | None = None,
    body: dict | None = None,
    expect_json: bool = True,
) -> dict:
    headers = {}
    if key is not None:
        headers["X-API-Key"] = key
    if body is not None:
        headers["Content-Type"] = "application/json"
    data = json.dumps(body).encode() if body is not None else None
    with urllib.request.urlopen(
        urllib.request.Request(url, data=data, headers=headers, method=method),
        timeout=2,
    ) as response:
        if response.status != (201 if method == "POST" else 200):
            raise RuntimeError(f"Unexpected API status: {response.status}")
        return json.load(response) if expect_json else {}


def start_api(port: str) -> str:
    global APP_PROCESS, APP_LOG
    APP_LOG = tempfile.NamedTemporaryFile(  # noqa: SIM115 - closed by stop_api
        prefix="com03-api-", suffix=".log", delete=False
    )
    env = os.environ.copy()
    env["PYTHONPATH"] = str(ROOT / "src")
    APP_PROCESS = subprocess.Popen(
        [
            sys.executable,
            "-m",
            "uvicorn",
            "event_service.main:app",
            "--host",
            "127.0.0.1",
            "--port",
            port,
        ],
        cwd=ROOT,
        env=env,
        stdout=APP_LOG,
        stderr=subprocess.STDOUT,
    )
    url = f"http://127.0.0.1:{port}"
    deadline = time.monotonic() + 45
    while time.monotonic() < deadline:
        if APP_PROCESS.poll() is not None:
            raise RuntimeError(f"API exited; inspect {APP_LOG.name}")
        try:
            request("GET", f"{url}/health/ready", expect_json=False)
            return url
        except (urllib.error.URLError, TimeoutError, ValueError):
            time.sleep(0.5)
    raise RuntimeError(f"API did not become ready; inspect {APP_LOG.name}")


def stop_api() -> None:
    global APP_PROCESS, APP_LOG
    if APP_PROCESS is not None:
        APP_PROCESS.terminate()
        try:
            APP_PROCESS.wait(timeout=10)
        except subprocess.TimeoutExpired:
            APP_PROCESS.kill()
            APP_PROCESS.wait()
        APP_PROCESS = None
    if APP_LOG is not None:
        APP_LOG.close()
        APP_LOG = None


def main() -> None:
    project, port = configure()
    compose(project, "up", "-d", "postgres")
    try:
        wait_postgres(project)
        upgrade()
        api_url = start_api(port)
        external_id = uuid4().hex
        event = {
            "source": "com03-defense",
            "external_id": external_id,
            "type": "temperature",
            "occurred_at": "2026-10-21T12:00:00Z",
            "payload": {"value": 23.5},
        }
        created = request(
            "POST",
            f"{api_url}/v1/events",
            key=os.environ["EVENT_PRODUCER_KEY"],
            body=event,
        )
        event_id = created["id"]
        stop_api()
        compose(
            project, "stop", "postgres"
        )  # Never down -v: the same named volume survives.
        compose(project, "start", "postgres")
        wait_postgres(project)
        upgrade()
        upgrade()  # A second upgrade to head is a no-op.
        api_url = start_api(port)
        restored = request(
            "GET",
            f"{api_url}/v1/events/{event_id}",
            key=os.environ["EVENT_ANALYST_KEY"],
        )
        if (
            restored.get("id") != event_id
            or any(
                restored.get(key) != value
                for key, value in event.items()
                if key != "occurred_at"
            )
            or datetime.fromisoformat(restored["occurred_at"]).astimezone(UTC)
            != datetime.fromisoformat(event["occurred_at"])
        ):
            raise RuntimeError("Event contents changed after restarting both processes")
        summary = request(
            "GET",
            f"{api_url}/v1/analysis/summary?from=2026-10-21T00:00:00Z&to=2026-10-22T00:00:00Z&source=com03-defense",
            key=os.environ["EVENT_ANALYST_KEY"],
        )
        if summary.get("total_events") != 1:
            raise RuntimeError("Summary query failed to reflect persisted event")
        print(f"PASS: event {event_id} persisted after API + PostgreSQL restart")
    finally:
        stop_api()
        compose(
            project, "stop", "postgres"
        )  # Stops the test container; retains the named volume.


if __name__ == "__main__":
    main()
