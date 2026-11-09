#!/bin/bash
# DEV-05: entrega un digest a un ambiente: migración, rollout, verificación y smoke.
# Si el rollout o el smoke fallan, vuelve a la versión anterior y termina con error.
# Las escrituras usan event-service-deployer, que no puede leer Secrets. Solo el smoke,
# que necesita las claves de API, usa la identidad con la que corre este script.
#
# Uso: scripts/k8s-deliver.sh <development|production> <imagen@sha256:...> <dir-manifiestos>
set -euo pipefail

environment=${1:-}
image=${2:-}
manifests=${3:-}
case "$environment" in
  development | production) ;;
  *)
    echo "Uso: $0 <development|production> <imagen@sha256:...> <dir-manifiestos>" >&2
    exit 2
    ;;
esac
if ! grep -Eq '^[a-z0-9./-]+@sha256:[0-9a-f]{64}$' <<<"$image"; then
  echo "La imagen debe indicarse por digest." >&2
  exit 2
fi

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
namespace="eventos-$environment"
read -r -a kubectl_cmd <<<"${KUBECTL:-kubectl}"
kube() { "${kubectl_cmd[@]}" "$@"; }
deployer() { kube --as="system:serviceaccount:$namespace:event-service-deployer" -n "$namespace" "$@"; }

deployments=(event-service-api event-service-worker)
previous=$(deployer get deployment event-service-api --ignore-not-found \
  -o jsonpath='{.spec.template.spec.containers[0].image}')

roll_back() {
  if [[ -z "$previous" || "$previous" == "$image" ]]; then
    echo "No hay una versión anterior distinta a la cual volver." >&2
    return
  fi
  echo "Volviendo a $previous" >&2
  for deployment in "${deployments[@]}"; do
    deployer rollout undo "deployment/$deployment" || true
    deployer rollout status "deployment/$deployment" --timeout=180s || true
  done
}

wait_for_job() {
  local job=$1 state
  for _ in $(seq 1 80); do
    state=$(deployer get job "$job" -o jsonpath='{.status.conditions[?(@.status=="True")].type}')
    case "$state" in
      *Complete*) return 0 ;;
      *Failed*) return 1 ;;
    esac
    sleep 3
  done
  return 1
}

pods_run_image() {
  local component=$1 expected=${image##*@} found
  found=$(deployer get pods -l "app.kubernetes.io/component=$component" \
    --field-selector=status.phase=Running \
    -o jsonpath='{range .items[*]}{.status.containerStatuses[0].imageID}{"\n"}{end}')
  [[ -n "$found" ]] && ! grep -v "@$expected\$" <<<"$found" | grep -q .
}

job=$(grep -Eo 'event-service-migrate-[0-9a-f]{16}' "$manifests/migration.json" | head -1)
# El Job no reintenta: si la base aún arranca, por ejemplo tras encender la VM, fallaría.
deployer wait --for=condition=Ready pod/postgres-0 --timeout=180s >/dev/null

echo "== Migración: $job"
deployer apply --dry-run=server -f "$manifests/migration.json" >/dev/null
deployer apply -f "$manifests/migration.json" >/dev/null
if ! wait_for_job "$job"; then
  echo "La migración falló: no se actualizan los workloads." >&2
  deployer logs "job/$job" --tail=20 >&2 || true
  exit 1
fi

echo "== Rollout de $image"
deployer apply --dry-run=server -f "$manifests/workloads.json" >/dev/null
deployer apply -f "$manifests/workloads.json" >/dev/null
for deployment in "${deployments[@]}"; do
  if ! deployer rollout status "deployment/$deployment" --timeout=180s; then
    echo "El rollout de $deployment falló." >&2
    roll_back
    exit 1
  fi
done

echo "== Versión activa"
# El Pod anterior puede seguir terminando un momento después del rollout.
for component in api worker; do
  verified=false
  for _ in $(seq 1 20); do
    if pods_run_image "$component"; then
      verified=true
      break
    fi
    sleep 3
  done
  if [[ "$verified" != true ]]; then
    echo "Los Pods de $component no ejecutan el digest esperado." >&2
    roll_back
    exit 1
  fi
done

echo "== Smoke"
if ! KUBECTL="${kubectl_cmd[*]}" bash "$script_dir/k8s-smoke.sh" "$environment"; then
  roll_back
  exit 1
fi

echo "ENTREGA_OK $environment $image"
