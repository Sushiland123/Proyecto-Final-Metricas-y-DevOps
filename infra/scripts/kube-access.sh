#!/usr/bin/env bash
# Obtiene el kubeconfig de K3s para administrar el clúster por un túnel SSH.
# La API de Kubernetes (6443) no está expuesta a Internet.
#   1) bash infra/scripts/kube-access.sh            -> escribe ./kubeconfig (ignorado por Git)
#   2) ssh -N -L 6443:127.0.0.1:6443 <admin>@<fqdn> (en otra terminal)
#   3) KUBECONFIG=./kubeconfig kubectl get nodes
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FQDN="$(terraform -chdir="$ROOT/infra/terraform" output -raw public_fqdn)"
ADMIN="${ADMIN_USER:-azureuser}"
OUT="${1:-$ROOT/kubeconfig}"

ssh -o StrictHostKeyChecking=accept-new "${ADMIN}@${FQDN}" 'cat ~/.kube/config' > "$OUT"
chmod 600 "$OUT"
echo "kubeconfig guardado en $OUT (servidor https://127.0.0.1:6443)."
echo "Abrir el túnel en otra terminal: ssh -N -L 6443:127.0.0.1:6443 ${ADMIN}@${FQDN}"
