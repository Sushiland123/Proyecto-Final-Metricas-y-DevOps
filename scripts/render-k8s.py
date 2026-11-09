#!/usr/bin/env python3
"""Render one explicit deployment stage to JSON. Never contacts a cluster."""

import argparse
import json
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
APP = "ghcr.io/marchelo23/proyecto-final-metricas-y-devops/event-service"
POSTGRES = "docker.io/library/postgres"


def render(environment, stage, image, postgres_image):
    if environment not in {"development", "production"}:
        raise ValueError("unknown environment")
    if stage not in {"foundation", "migration", "workloads"}:
        raise ValueError("unknown stage; render and apply each stage in order")
    for value, repository in ((image, APP), (postgres_image, POSTGRES)):
        if not re.fullmatch(re.escape(repository) + r"@sha256:[0-9a-f]{64}", value):
            raise ValueError("only approved repositories with immutable sha256 digests are accepted")
    namespace = f"eventos-{environment}"
    replacements = {
        "$NAMESPACE": namespace, "$IMAGE": image, "$POSTGRES_IMAGE": postgres_image,
        "$MIGRATION_JOB": "event-service-migrate-" + image.rsplit(":", 1)[1][:16],
        "$POSTGRES_INIT_SCRIPT": (ROOT / "kubernetes/postgres-init.sh").read_text(encoding="utf-8"),
    }

    def expand(value):
        if isinstance(value, str):
            return replacements.get(value, value)
        if isinstance(value, list):
            return [expand(item) for item in value]
        if isinstance(value, dict):
            return {key: expand(item) for key, item in value.items()}
        return value

    source = json.loads((ROOT / "kubernetes/resources.json").read_text(encoding="utf-8"))
    items = expand(source[stage])
    for item in items:
        if item["kind"] != "Namespace":
            item["metadata"]["namespace"] = namespace
        item["metadata"].setdefault("labels", {})["app.kubernetes.io/part-of"] = "eventos"
        item["metadata"]["labels"]["app.kubernetes.io/instance"] = namespace
    return {"apiVersion": "v1", "kind": "List", "items": items}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--environment", required=True, choices=["development", "production"])
    parser.add_argument("--stage", required=True, choices=["foundation", "migration", "workloads"])
    parser.add_argument("--image", required=True)
    parser.add_argument("--postgres-image", required=True)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        encoded = json.dumps(render(args.environment, args.stage, args.image, args.postgres_image), indent=2) + "\n"
        if args.output:
            args.output.write_text(encoded, encoding="utf-8")
        else:
            print(encoded, end="")
        return 0
    except (ValueError, OSError) as error:
        print(f"Cannot render deployment ({type(error).__name__}); no resources applied.", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
