#!/bin/bash
# DEV-12: simulacro de despliegue defectuoso. Solo se ejecuta contra development.
# Provoca dos fallas solo en la API, una de imagen y otra de configuración, mide la
# recuperación y comprueba que el worker no se modifica.
#
# Uso: scripts/k8s-rollback-drill.sh
set -euo pipefail

environment=development
namespace="eventos-$environment"
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
read -r -a kubectl_cmd <<<"${KUBECTL:-kubectl}"
kube() { "${kubectl_cmd[@]}" "$@"; }
deployer() { kube --as="system:serviceaccount:$namespace:event-service-deployer" -n "$namespace" "$@"; }

api=deployment/event-service-api
active_image() { deployer get "$api" -o jsonpath='{.spec.template.spec.containers[0].image}'; }

readings_count() {
  # Solo imprime un número. Las credenciales se expanden dentro del contenedor.
  # shellcheck disable=SC2016
  kube -n "$namespace" exec postgres-0 -- sh -ec \
    'PGPASSWORD="$POSTGRES_PASSWORD" psql -h 127.0.0.1 -U "$POSTGRES_USER" -d "$POSTGRES_DB" -tAc "select count(*) from readings"'
}

expect_failed_rollout() {
  if deployer rollout status "$api" --timeout=75s; then
    echo "La falla provocada no se detectó." >&2
    exit 1
  fi
  echo "Detectado: el rollout no se completó."
  deployer get pods -l app.kubernetes.io/component=api
}

recover_and_compare() {
  local started=$1
  KUBECTL="${kubectl_cmd[*]}" bash "$script_dir/k8s-rollback.sh" "$environment" "$good_image"
  [[ "$(active_image)" == "$good_image" ]] || {
    echo "La versión activa no es la esperada." >&2
    exit 1
  }
  [[ "$(worker_state)" == "$worker_before" ]] || {
    echo "El rollback modificó el worker, que no había cambiado." >&2
    exit 1
  }
  echo "Worker intacto: $worker_before"
  echo "Tiempo total desde la falla: $(($(date +%s) - started)) s"
}

# Generación, revisión, imagen y Pod: cualquier cambio del worker altera este valor.
worker_state() {
  deployer get deployment/event-service-worker -o \
    jsonpath='{.metadata.generation} {.metadata.annotations.deployment\.kubernetes\.io/revision} {.spec.template.spec.containers[0].image} '
  deployer get pods -l app.kubernetes.io/component=worker --field-selector=status.phase=Running \
    -o jsonpath='{.items[*].metadata.name}'
}

KUBECTL="${kubectl_cmd[*]}" bash "$script_dir/k8s-smoke.sh" "$environment"
good_image=$(active_image)
before=$(readings_count)
worker_before=$(worker_state)
echo "Versión sana: $good_image"
echo "Lecturas antes del simulacro: $before"

echo
echo "== Falla 1: imagen inexistente"
started=$(date +%s)
deployer patch "$api" --type json -p "[{\"op\":\"replace\",\"path\":\"/spec/template/spec/containers/0/image\",\"value\":\"${good_image%@*}@sha256:$(printf '0%.0s' {1..64})\"}]" >/dev/null
expect_failed_rollout
recover_and_compare "$started"

echo
echo "== Falla 2: configuración inválida"
started=$(date +%s)
deployer patch "$api" --type strategic -p '{"spec":{"template":{"spec":{"containers":[{"name":"api","env":[{"name":"BIND_ADDR","value":"direccion-invalida"}]}]}}}}' >/dev/null
expect_failed_rollout
recover_and_compare "$started"

after=$(readings_count)
echo
echo "Lecturas después del simulacro: $after"
if [[ "$after" -lt "$before" ]]; then
  echo "Se perdieron lecturas." >&2
  exit 1
fi
echo "SIMULACRO_OK: versión $good_image, sin pérdida de lecturas."
