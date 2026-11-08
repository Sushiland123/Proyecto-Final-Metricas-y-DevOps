#!/usr/bin/env bash
# Obtiene el kubeconfig de K3s para administrar el clúster por un túnel SSH.
# La API de Kubernetes (6443) no está expuesta a Internet.
#   1) bash infra/scripts/kube-access.sh            -> escribe ./kubeconfig (ignorado por Git)
#   2) ssh -N -L 6443:127.0.0.1:6443 <admin>@<fqdn> (en otra terminal)
#   3) KUBECONFIG=./kubeconfig kubectl get nodes
# Con una clave distinta de la predeterminada: SSH_KEY=~/.ssh/<clave> bash infra/scripts/kube-access.sh
# Antes de la primera conexión, comparar la huella con: bash infra/scripts/vm.sh fingerprint
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FQDN="$(terraform -chdir="$ROOT/infra/terraform" output -raw public_fqdn)"
ADMIN="${ADMIN_USER:-azureuser}"
OUT="${1:-$ROOT/kubeconfig}"

KEY_OPT=""
[[ -n "${SSH_KEY:-}" ]] && KEY_OPT="-i ${SSH_KEY}"

# shellcheck disable=SC2086
ssh $KEY_OPT -o StrictHostKeyChecking=accept-new "${ADMIN}@${FQDN}" 'cat ~/.kube/config' > "$OUT"
chmod 600 "$OUT"
echo "kubeconfig guardado en $OUT (servidor https://127.0.0.1:6443)."
echo "Abrir el túnel en otra terminal: ssh ${KEY_OPT:+$KEY_OPT }-N -L 6443:127.0.0.1:6443 ${ADMIN}@${FQDN}"
