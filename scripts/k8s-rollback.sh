#!/bin/bash
# DEV-12: vuelve API y worker a una versión sana, identificada por digest, y la comprueba.
# Un Deployment que ya está sano en esa versión no se modifica. No revierte migraciones
# ni restaura datos.
#
# Uso: scripts/k8s-rollback.sh <development|production> <imagen@sha256:...>
#      Sin imagen, lista las versiones que cada Deployment conserva.
set -euo pipefail

environment=${1:-}
target=${2:-}
case "$environment" in
  development | production) ;;
  *)
    echo "Uso: $0 <development|production> <imagen@sha256:...>" >&2
    exit 2
    ;;
esac

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
namespace="eventos-$environment"
read -r -a kubectl_cmd <<<"${KUBECTL:-kubectl}"
deployer() {
  "${kubectl_cmd[@]}" --as="system:serviceaccount:$namespace:event-service-deployer" -n "$namespace" "$@"
}

deployments=(event-service-api event-service-worker)
revision_path='{.metadata.annotations.deployment\.kubernetes\.io/revision}'
image_path='{.spec.template.spec.containers[0].image}'

# Imprime "<revisión> <imagen>" de cada versión que conserva un componente.
kept_versions() {
  deployer get replicasets -l "app.kubernetes.io/component=$1" \
    -o jsonpath="{range .items[*]}$revision_path{\" \"}$image_path{\"\\n\"}{end}" | sort -n
}

if [[ -z "$target" ]]; then
  for component in api worker; do
    echo "== $component (revisión e imagen)"
    kept_versions "$component"
  done
  echo "Indique la imagen sana: $0 $environment <imagen@sha256:...>" >&2
  exit 2
fi
if ! grep -Eq '^[a-z0-9./-]+@sha256:[0-9a-f]{64}$' <<<"$target"; then
  echo "La imagen debe indicarse por digest." >&2
  exit 2
fi

# La imagen no basta: una revisión puede tener el digest correcto y una configuración mala.
is_healthy_on_target() {
  local generation observed wanted total updated available
  [[ "$(deployer get "deployment/$1" -o jsonpath="$image_path")" == "$target" ]] || return 1
  read -r generation observed wanted total updated available <<<"$(deployer get "deployment/$1" -o \
    jsonpath='{.metadata.generation} {.status.observedGeneration} {.spec.replicas} {.status.replicas} {.status.updatedReplicas} {.status.availableReplicas}')"
  [[ "$generation" == "$observed" && "$total" == "$wanted" && "$updated" == "$wanted" && "$available" == "$wanted" ]]
}

# La revisión más reciente con ese digest, distinta de la actual.
revision_to_restore() {
  local current
  current=$(deployer get "deployment/$1" -o jsonpath="$revision_path")
  kept_versions "${1#event-service-}" | awk -v image="$target" -v current="$current" \
    '$2 == image && $1 != current { found = $1 } END { print found }'
}

pods_run_target() {
  local found
  found=$(deployer get pods -l "app.kubernetes.io/component=$1" --field-selector=status.phase=Running \
    -o jsonpath='{range .items[*]}{.status.containerStatuses[0].imageID}{"\n"}{end}')
  [[ -n "$found" ]] && ! grep -v "@${target##*@}\$" <<<"$found" | grep -q .
}

started=$(date +%s)
for deployment in "${deployments[@]}"; do
  if is_healthy_on_target "$deployment"; then
    echo "$deployment ya está sano en esa versión: no se modifica."
    continue
  fi
  revision=$(revision_to_restore "$deployment")
  if [[ -z "$revision" ]]; then
    echo "$deployment no conserva otra revisión con esa imagen." >&2
    exit 1
  fi
  echo "Restaurando $deployment a la revisión $revision."
  deployer rollout undo "deployment/$deployment" --to-revision="$revision"
done

for deployment in "${deployments[@]}"; do
  deployer rollout status "deployment/$deployment" --timeout=180s
  active=$(deployer get "deployment/$deployment" -o jsonpath="$image_path")
  if [[ "$active" != "$target" ]]; then
    echo "$deployment quedó en $active y no en la versión pedida." >&2
    exit 1
  fi
  verified=false
  for _ in $(seq 1 20); do
    if pods_run_target "${deployment#event-service-}"; then
      verified=true
      break
    fi
    sleep 3
  done
  if [[ "$verified" != true ]]; then
    echo "Los Pods de $deployment no ejecutan el digest pedido." >&2
    exit 1
  fi
done

KUBECTL="${kubectl_cmd[*]}" bash "$script_dir/k8s-smoke.sh" "$environment"
echo "ROLLBACK_OK $environment $target en $(($(date +%s) - started)) s"
