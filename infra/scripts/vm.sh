#!/usr/bin/env bash
# Operación de costo de la VM de K3s (DEV-14).
#   bash infra/scripts/vm.sh status   estado de energía
#   bash infra/scripts/vm.sh start    encender antes de trabajar o de una demostración
#   bash infra/scripts/vm.sh stop     apagar y DESASIGNAR (deja de cobrarse el cómputo)
#   bash infra/scripts/vm.sh fingerprint  huella SSH del servidor, leída por el canal de Azure
# Un "stop" desde el sistema operativo no desasigna: la VM sigue cobrando.
set -euo pipefail
export MSYS_NO_PATHCONV=1

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck disable=SC1090
source "$ROOT/infra/bootstrap/bootstrap.env"
RG="rg-eventos-${NAME_SUFFIX}"
VM="vm-eventos-${NAME_SUFFIX}-k3s"

power_state() {
  az vm get-instance-view -g "$RG" -n "$VM" \
    --query "instanceView.statuses[?starts_with(code, 'PowerState/')].displayStatus | [0]" -o tsv
}

case "${1:-status}" in
  status) echo "$VM: $(power_state)" ;;
  start) az vm start -g "$RG" -n "$VM" -o none && echo "$VM: $(power_state)" ;;
  stop) az vm deallocate -g "$RG" -n "$VM" -o none && echo "$VM: $(power_state)" ;;
  fingerprint)
    # Comparar con la huella que muestra ssh en la primera conexión.
    az vm run-command invoke -g "$RG" -n "$VM" --command-id RunShellScript       --scripts "ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub" --query "value[0].message" -o tsv ;;
  *) echo "Uso: $0 status|start|stop|fingerprint" >&2; exit 1 ;;
esac
