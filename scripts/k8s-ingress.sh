#!/bin/bash
# DEV-05: publica producción por HTTPS. Instala cert-manager en una versión y checksum
# fijos y aplica el Ingress. Lo ejecuta un administrador una sola vez; es idempotente.
#
# Uso: scripts/k8s-ingress.sh <fqdn-publico>
#      KUBECTL="sudo k3s kubectl" scripts/k8s-ingress.sh eventos-<sufijo>.mexicocentral.cloudapp.azure.com
set -euo pipefail

public_host=${1:-}
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
read -r -a kubectl_cmd <<<"${KUBECTL:-kubectl}"
kube() { "${kubectl_cmd[@]}" "$@"; }

CERT_MANAGER_VERSION=v1.21.3
CERT_MANAGER_SHA256=c04a51295adda2b1a775146751f973e787190778c7f35180861d9678876e5c74
# La etapa ingress no usa estas imágenes; estos digests solo satisfacen al renderer.
placeholder="ghcr.io/marchelo23/proyecto-final-metricas-y-devops/event-service@sha256:$(printf '0%.0s' {1..64})"
postgres_placeholder="docker.io/library/postgres@sha256:$(printf '0%.0s' {1..64})"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Primero se valida el nombre: así no se instala nada si el argumento es incorrecto.
python3 "$root/scripts/render-k8s.py" --environment production --stage ingress \
  --image "$placeholder" --postgres-image "$postgres_placeholder" \
  --public-host "$public_host" --output "$work/ingress.json"

curl --fail --location --silent --show-error --output "$work/cert-manager.yaml" \
  "https://github.com/cert-manager/cert-manager/releases/download/$CERT_MANAGER_VERSION/cert-manager.yaml"
echo "$CERT_MANAGER_SHA256  $work/cert-manager.yaml" | sha256sum --check --quiet

kube apply -f - <"$work/cert-manager.yaml" >/dev/null
for deployment in cert-manager cert-manager-cainjector cert-manager-webhook; do
  kube -n cert-manager rollout status "deployment/$deployment" --timeout=240s
done

# El webhook tarda unos segundos en aceptar peticiones después de estar disponible.
for attempt in $(seq 1 20); do
  if kube apply -f - <"$work/ingress.json"; then
    echo "Ingress aplicado. El certificado se emite cuando Let's Encrypt valida el dominio."
    exit 0
  fi
  echo "Reintento $attempt: el webhook aún no responde." >&2
  sleep 6
done
exit 1
