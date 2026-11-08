#!/usr/bin/env bash
# Comprueba el endurecimiento de una imagen ya construida, busca vulnerabilidades
# críticas y genera los SBOM. CI ejecuta este mismo script.
#   docker build -t event-service:local .
#   bash scripts/verify-image.sh event-service:local
# Resultados en artifacts/image/ (ignorado por Git).
set -euo pipefail

export MSYS_NO_PATHCONV=1 # Git Bash en Windows no debe reescribir las rutas del contenedor
TRIVY_IMAGE="aquasec/trivy:0.76.2@sha256:8f6bb947dbc528902e11f2f5be52f7b519f6a39d5d148f41166f2040499768fb"

IMAGE="${1:?Uso: verify-image.sh <imagen>}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
HOST_ROOT="$(pwd -W 2>/dev/null || pwd)"
OUT="artifacts/image"
mkdir -p "$OUT"

# Ejecuta una comprobación y sigue con las demás, para informar todas las fallas.
failures=0
check() {
  local description="$1"
  shift
  if "$@"; then
    echo "OK     $description"
  else
    echo "FALLA  $description"
    failures=$((failures + 1))
  fi
}

user="$(docker inspect --format '{{.Config.User}}' "$IMAGE")"
user_is_numeric_non_root() { [[ "${user%%:*}" =~ ^[1-9][0-9]*$ ]]; }
has_no_shell() { ! docker run --rm --entrypoint /bin/sh "$IMAGE" -c true >/dev/null 2>&1; }
serves_openapi() { docker run --rm "$IMAGE" openapi >/dev/null 2>&1; }
label_of() { docker inspect --format "{{index .Config.Labels \"org.opencontainers.image.$1\"}}" "$IMAGE"; }
has_label() { [[ -n "$(label_of "$1")" ]]; }
has_no_secret_env() {
  ! docker inspect --format '{{range .Config.Env}}{{println .}}{{end}}' "$IMAGE" |
    grep -Eq '^(DATABASE_URL|POSTGRES_PASSWORD|EVENT_[A-Z_]*KEY[A-Z_]*)='
}

check "usuario numérico distinto de root (${user:-sin definir})" user_is_numeric_non_root
check "la imagen no tiene shell" has_no_shell
check "el binario arranca y entrega el contrato OpenAPI" serves_openapi
for label in source revision version; do
  check "label OCI $label = $(label_of "$label")" has_label "$label"
done
check "la imagen no define variables de secretos" has_no_secret_env

# La imagen se analiza desde un tar para no montar el socket de Docker en Trivy.
docker save "$IMAGE" -o "$OUT/image.tar"
trivy() {
  docker run --rm -v trivy-cache:/root/.cache/trivy -v "$HOST_ROOT:/work" -w /work "$TRIVY_IMAGE" "$@"
}
# El binario va sin símbolos y Trivy no ve sus crates: esas se analizan desde Cargo.lock.
rust_scan() {
  docker run --rm -v trivy-cache:/root/.cache/trivy -v "$HOST_ROOT/Cargo.lock:/src/Cargo.lock:ro" \
    -v "$HOST_ROOT:/work" -w /work "$TRIVY_IMAGE" fs "$@" /src
}

trivy image --quiet --input "$OUT/image.tar" --format table --severity HIGH,CRITICAL \
  --output "$OUT/vulnerabilidades-imagen.txt"
rust_scan --quiet --scanners vuln --format table --severity HIGH,CRITICAL \
  --output "$OUT/vulnerabilidades-rust.txt"
trivy image --quiet --input "$OUT/image.tar" --format cyclonedx --output "$OUT/sbom-imagen.cdx.json"
rust_scan --quiet --format cyclonedx --output "$OUT/sbom-rust.cdx.json"

image_has_no_critical() {
  trivy image --quiet --input "$OUT/image.tar" --severity CRITICAL --ignore-unfixed --exit-code 1 >/dev/null
}
crates_have_no_critical() {
  rust_scan --quiet --scanners vuln --severity CRITICAL --ignore-unfixed --exit-code 1 >/dev/null
}
check "imagen sin vulnerabilidades críticas con parche disponible" image_has_no_critical
check "crates de Rust sin vulnerabilidades críticas con parche disponible" crates_have_no_critical

rm -f "$OUT/image.tar"

if [[ "$failures" -gt 0 ]]; then
  echo "$failures comprobaciones fallaron."
  exit 1
fi
echo "Imagen verificada: $IMAGE"
