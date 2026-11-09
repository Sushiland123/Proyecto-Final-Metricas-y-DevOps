#!/usr/bin/env python3
"""Integration test in an owned, network-isolated, disposable local container."""

import os
from pathlib import Path
import secrets
import subprocess
import time
import uuid


def main():
    name = "dev09-init-" + uuid.uuid4().hex[:12]
    volume = name + "-data"
    env = os.environ.copy()
    env.update(POSTGRES_USER="bootstrap_admin", POSTGRES_DB="eventos_test")
    password_names = ("POSTGRES_PASSWORD", "EVENT_MIGRATION_PASSWORD", "EVENT_RUNTIME_PASSWORD")
    for key in password_names:
        env[key] = secrets.token_hex(24)

    def docker(*args, check=True):
        result = subprocess.run(["docker", *args], env=env, capture_output=True, text=True, timeout=60)
        if check and result.returncode:
            # Do not print database startup logs or secret values on failure.
            raise RuntimeError(f"local Docker operation failed: {args[0]} (exit {result.returncode})")
        return result

    created_volume = False
    try:
        docker("volume", "create", volume)
        created_volume = True
        command = [
            "run", "-d", "--network", "none", "--name", name, "--user", "999:999", "--read-only",
            "--tmpfs", "/tmp:mode=1777", "--tmpfs", "/var/run/postgresql:mode=1777",
            "-v", volume + ":/var/lib/postgresql/data",
            "-v", str(Path(__file__).resolve().parents[1] / "kubernetes/postgres-init.sh")
            + ":/docker-entrypoint-initdb.d/10-runtime-roles.sh:ro",
        ]
        for key in ("POSTGRES_USER", "POSTGRES_DB", *password_names):
            command.extend(["-e", key])
        command.append(os.environ.get("POSTGRES_TEST_IMAGE", "docker.io/library/postgres@sha256:5264710be20f409750ac6dc4e5706dcef929c87e241605463f0f9aefa932a8d9"))
        docker(*command)
        for _ in range(60):
            result = docker("exec", name, "pg_isready", "-h", "127.0.0.1", "-U", "bootstrap_admin", "-d", "eventos_test", check=False)
            if result.returncode == 0:
                break
            time.sleep(1)
        else:
            raise RuntimeError("PostgreSQL did not start; no acceptance recorded")

        def sql(role, statement, check=True):
            return docker("exec", name, "psql", "-v", "ON_ERROR_STOP=1", "-U", role, "-d", "eventos_test", "-c", statement, check=check)

        sql("eventos_migrator", "CREATE TABLE deployment_probe (id integer primary key, value integer);")
        sql("eventos_runtime", "INSERT INTO deployment_probe VALUES (1, 1); UPDATE deployment_probe SET value=2; SELECT * FROM deployment_probe;")
        for statement in ("CREATE TABLE forbidden_probe (id integer);", "DELETE FROM deployment_probe;"):
            denied = sql("eventos_runtime", statement, check=False)
            if denied.returncode == 0 or "permission denied" not in denied.stderr:
                raise RuntimeError("runtime role was not denied the expected privilege")
        print("PASS: PostgreSQL 16 init as UID 999 with read-only rootfs and writable data/run/tmp")
        print("PASS: migrator DDL; runtime SELECT/INSERT/UPDATE allowed, DDL/DELETE denied")
    finally:
        # Only the exact randomly named resources owned by this invocation.
        if created_volume:
            docker("rm", "--force", name, check=False)
            docker("volume", "rm", volume, check=False)


if __name__ == "__main__":
    main()
