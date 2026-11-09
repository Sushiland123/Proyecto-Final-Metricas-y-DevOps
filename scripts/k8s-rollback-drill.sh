#!/bin/bash
# DEV-12: simulacro de despliegue defectuoso. Solo se ejecuta contra development.
# Provoca dos fallas, una de imagen y otra de configuración, y mide la recuperación.
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
  KUBECTL="${kubectl_cmd[*]}" bash "$script_dir/k8s-rollback.sh" "$environment" 2>&1 | tail -2
  [[ "$(active_image)" == "$good_image" ]] || {
    echo "La versión activa no es la esperada." >&2
    exit 1
  }
  echo "Tiempo total desde la falla: $(($(date +%s) - started)) s"
}

# El rollback de este simulacro revierte también al worker, así que parte de una misma
# revisión anterior: se registra una revisión sana antes de provocar cada falla.
checkpoint_worker() {
  local stamp
  stamp=$(date +%s)
  deployer patch deployment/event-service-worker --type merge \
    -p "{\"spec\":{\"template\":{\"metadata\":{\"annotations\":{\"simulacro/punto\":\"$stamp\"}}}}}" >/dev/null
  deployer rollout status deployment/event-service-worker --timeout=180s >/dev/null
}

KUBECTL="${kubectl_cmd[*]}" bash "$script_dir/k8s-smoke.sh" "$environment"
good_image=$(active_image)
before=$(readings_count)
echo "Versión sana: $good_image"
echo "Lecturas antes del simulacro: $before"

echo
echo "== Falla 1: imagen inexistente"
checkpoint_worker
started=$(date +%s)
deployer patch "$api" --type json -p "[{\"op\":\"replace\",\"path\":\"/spec/template/spec/containers/0/image\",\"value\":\"${good_image%@*}@sha256:$(printf '0%.0s' {1..64})\"}]" >/dev/null
expect_failed_rollout
recover_and_compare "$started"

echo
echo "== Falla 2: configuración inválida"
checkpoint_worker
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
