#!/usr/bin/env bash
# Busca secretos en todo el historial de Git, incluidas todas las ramas locales.
# La salida va censurada. Termina con error si hay hallazgos.
#   bash scripts/scan-secrets.sh
set -euo pipefail

export MSYS_NO_PATHCONV=1 # Git Bash en Windows no debe reescribir las rutas del contenedor
GITLEAKS_IMAGE="ghcr.io/gitleaks/gitleaks:v8.28.0@sha256:cdbb7c955abce02001a9f6c9f602fb195b7fadc1e812065883f695d1eeaba854"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
HOST_ROOT="$(pwd -W 2>/dev/null || pwd)"

docker run --rm -v "$HOST_ROOT:/repo:ro" "$GITLEAKS_IMAGE" \
  git /repo --no-banner --redact --log-opts="--all" --config /repo/.gitleaks.toml
