#!/bin/bash
# DEV-05: entrega un digest a un ambiente: migración, rollout, verificación y smoke.
# Si el rollout o el smoke fallan, restaura solo lo que esta entrega cambió y termina con error.
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

template_of() {
  deployer get deployment "$1" --ignore-not-found -o json | python3 -c '
import json, sys
deployment = json.loads(sys.stdin.read() or "{}")
template = deployment.get("spec", {}).get("template")
print(json.dumps(template, sort_keys=True) if template is not None else "")'
}

# El template real cambia antes de que el controlador publique una revisión nueva.
# Capturarlo permite restaurar incluso un apply parcial con anotaciones todavía viejas.
declare -A previous_template previous_image
for deployment in "${deployments[@]}"; do
  previous_template[$deployment]=$(template_of "$deployment")
  previous_image[$deployment]=$(printf '%s' "${previous_template[$deployment]}" | python3 -c '
import json, sys
template = json.loads(sys.stdin.read() or "{}")
containers = template.get("spec", {}).get("containers", [])
print(containers[0]["image"] if containers else "")')
done

# Un Deployment que esta entrega no cambió se deja como está: deshacerlo lo haría retroceder.
roll_back() {
  local deployment template current restored=false failed=false
  for deployment in "${deployments[@]}"; do
    template=${previous_template[$deployment]}
    if [[ -z "$template" ]]; then
      echo "$deployment no existía antes de esta entrega: no hay a qué volver." >&2
      continue
    fi
    if ! current=$(template_of "$deployment"); then
      echo "No se pudo comprobar el template de $deployment." >&2
      failed=true
      continue
    fi
    if [[ "$current" == "$template" ]]; then
      echo "$deployment no cambió: se deja como está." >&2
      continue
    fi
    echo "Restaurando el template capturado de $deployment: ${previous_image[$deployment]}" >&2
    restored=true
    printf '[{"op":"replace","path":"/spec/template","value":%s}]' "$template" |
      deployer patch "deployment/$deployment" --type=json --patch-file=/dev/stdin || failed=true
    deployer rollout status "deployment/$deployment" --timeout=180s || failed=true
    wait_for_digest "${deployment#event-service-}" "${previous_image[$deployment]##*@}" || failed=true
  done
  if [[ "$restored" != true && "$failed" != true ]]; then
    return 0
  fi
  KUBECTL="${kubectl_cmd[*]}" bash "$script_dir/k8s-smoke.sh" "$environment" || failed=true
  if [[ "$failed" == true ]]; then
    echo "ROLLBACK_FALLÓ: la versión anterior no quedó comprobada. Revisar a mano." >&2
  else
    echo "Versión anterior restaurada y comprobada." >&2
  fi
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

pods_run_digest() {
  local component=$1 expected=$2 found
  found=$(deployer get pods -l "app.kubernetes.io/component=$component" \
    --field-selector=status.phase=Running \
    -o jsonpath='{range .items[*]}{.status.containerStatuses[0].imageID}{"\n"}{end}')
  [[ -n "$found" ]] && ! grep -v "@$expected\$" <<<"$found" | grep -q .
}

# El Pod anterior puede seguir terminando un momento después del rollout.
wait_for_digest() {
  for _ in $(seq 1 20); do
    pods_run_digest "$1" "$2" && return 0
    sleep 3
  done
  return 1
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
if ! deployer apply -f "$manifests/workloads.json" >/dev/null; then
  echo "No se pudieron aplicar los workloads." >&2
  roll_back
  exit 1
fi
for deployment in "${deployments[@]}"; do
  if ! deployer rollout status "deployment/$deployment" --timeout=180s; then
    echo "El rollout de $deployment falló." >&2
    roll_back
    exit 1
  fi
done

echo "== Versión activa"
for component in api worker; do
  if ! wait_for_digest "$component" "${image##*@}"; then
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
