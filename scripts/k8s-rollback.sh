#!/bin/bash
# DEV-12: vuelve API y worker a la revisión anterior y comprueba el resultado con el smoke.
# No revierte migraciones ni restaura datos.
#
# Uso: scripts/k8s-rollback.sh <development|production>
set -euo pipefail

case "${1:-}" in
  development | production) environment=$1 ;;
  *)
    echo "Uso: $0 <development|production>" >&2
    exit 2
    ;;
esac

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
namespace="eventos-$environment"
read -r -a kubectl_cmd <<<"${KUBECTL:-kubectl}"
deployer() {
  "${kubectl_cmd[@]}" --as="system:serviceaccount:$namespace:event-service-deployer" -n "$namespace" "$@"
}

started=$(date +%s)
for deployment in event-service-api event-service-worker; do
  deployer rollout undo "deployment/$deployment"
done
for deployment in event-service-api event-service-worker; do
  deployer rollout status "deployment/$deployment" --timeout=180s
done

KUBECTL="${kubectl_cmd[*]}" bash "$script_dir/k8s-smoke.sh" "$environment"
image=$(deployer get deployment event-service-api -o jsonpath='{.spec.template.spec.containers[0].image}')
echo "ROLLBACK_OK $environment $image en $(($(date +%s) - started)) s"
