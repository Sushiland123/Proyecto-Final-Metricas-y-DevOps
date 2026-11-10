#!/bin/bash
# Preparación única de un ambiente, ejecutada por un administrador: namespace, Secrets,
# permisos, red y PostgreSQL. Las entregas siguientes no necesitan estos permisos.
#
# Uso: scripts/k8s-bootstrap.sh <development|production>
#      KUBECTL="sudo k3s kubectl" scripts/k8s-bootstrap.sh development
set -euo pipefail

case "${1:-}" in
  development | production) environment=$1 ;;
  *)
    echo "Uso: $0 <development|production>" >&2
    exit 2
    ;;
esac

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
namespace="eventos-$environment"
read -r -a kubectl_cmd <<<"${KUBECTL:-kubectl}"
kube() { "${kubectl_cmd[@]}" "$@"; }

# La etapa foundation no usa la imagen de la aplicación; este digest solo satisface al renderer.
placeholder="ghcr.io/marchelo23/proyecto-final-metricas-y-devops/event-service@sha256:$(printf '0%.0s' {1..64})"
postgres_image=docker.io/library/postgres@sha256:5264710be20f409750ac6dc4e5706dcef929c87e241605463f0f9aefa932a8d9
foundation=$(mktemp)
trap 'rm -f "$foundation"' EXIT
python3 "$root/scripts/render-k8s.py" --environment "$environment" --stage foundation \
  --image "$placeholder" --postgres-image "$postgres_image" --output "$foundation"

# El namespace va primero: los Secrets deben existir antes de arrancar PostgreSQL.
python3 -c 'import json,sys; print(json.dumps(json.load(open(sys.argv[1]))["items"][0]))' "$foundation" | kube apply -f -
KUBECTL="${kubectl_cmd[*]}" sh "$root/scripts/k8s-secrets.sh" "$environment"
kube apply --dry-run=server -f - <"$foundation" >/dev/null
kube apply -f - <"$foundation"
kube -n "$namespace" rollout status statefulset/postgres --timeout=300s
echo "$namespace preparado."
