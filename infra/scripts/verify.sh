#!/usr/bin/env bash
# Verificación final del ambiente (DEV-13), después de crearlo o reconstruirlo.
#   SSH_KEY=~/.ssh/<clave> bash infra/scripts/verify.sh
# Comprueba en orden: outputs de Terraform, VM encendida, identidad SSH del
# servidor contra la huella que reporta Azure, cloud-init, nodo K3s, Pods del
# sistema, respuesta HTTP pública, API 6443 cerrada y SSH sin contraseña.
# Requiere terraform inicializado en infra/terraform, Azure CLI con sesión y la VM encendida.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ADMIN="${ADMIN_USER:-azureuser}"
FAILS=0
ok() { printf '  OK     %s\n' "$*"; }
fail() { printf '  FALLA  %s\n' "$*"; FAILS=$((FAILS + 1)); }

FQDN="$(terraform -chdir="$ROOT/infra/terraform" output -raw public_fqdn 2>/dev/null)"
IP="$(terraform -chdir="$ROOT/infra/terraform" output -raw public_ip 2>/dev/null)"
# Sin estos valores, las pruebas de red darían falsos positivos (por ejemplo, 6443 "cerrado").
if [[ -z "$FQDN" || -z "$IP" ]]; then
  echo "No hay outputs de Terraform (public_fqdn, public_ip): ejecutar terraform init y apply primero." >&2
  exit 1
fi
echo "Ambiente: ${FQDN} (${IP})"

state="$(bash "$ROOT/infra/scripts/vm.sh" status)"
if [[ "$state" == *"VM running"* ]]; then ok "$state"; else
  fail "$state (encender con: bash infra/scripts/vm.sh start)"
  exit 1
fi

# Identidad del servidor: la huella que reporta Azure debe coincidir con la que
# presenta el servidor. Se usa un known_hosts temporal: no se acepta a ciegas.
expected="$(bash "$ROOT/infra/scripts/vm.sh" fingerprint | grep -o 'SHA256:[^ ]*' | head -1)"
KH="$(mktemp)"
trap 'rm -f "$KH"' EXIT
ssh-keyscan -T 15 -t ed25519 "$FQDN" 2>/dev/null > "$KH"
got="$(ssh-keygen -lf "$KH" 2>/dev/null | awk '{print $2}')"
if [[ -n "$expected" && "$got" == "$expected" ]]; then ok "huella SSH coincide con Azure (${got})"; else
  fail "huella SSH: Azure=${expected:-?} servidor=${got:-?}"
  exit 1
fi

SSH=(ssh -o BatchMode=yes -o ConnectTimeout=20 -o ConnectionAttempts=3
  -o StrictHostKeyChecking=yes -o UserKnownHostsFile="$KH")
[[ -n "${SSH_KEY:-}" ]] && SSH+=(-i "$SSH_KEY")

remote="$("${SSH[@]}" "${ADMIN}@${FQDN}" '
  cloud-init status | sed "s/^status: //"
  sudo k3s kubectl get nodes --no-headers | awk "{print \$2}"
  sudo k3s kubectl get pods -A --no-headers | awk "{print \$4}" | sort | uniq -c | xargs
')" || {
  fail "SSH a ${ADMIN}@${FQDN}"
  exit 1
}
mapfile -t lines <<<"$remote"
[[ "${lines[0]:-}" == "done" ]] && ok "cloud-init: done" || fail "cloud-init: ${lines[0]:-sin respuesta}"
[[ "${lines[1]:-}" == "Ready" ]] && ok "nodo K3s: Ready" || fail "nodo K3s: ${lines[1]:-sin respuesta}"
pods="${lines[2]:-}"
if [[ -n "$pods" ]] && ! tr ' ' '\n' <<<"$pods" | grep -vqE '^([0-9]+|Running|Completed)$'; then
  ok "Pods: ${pods}"
else
  fail "Pods con estados distintos de Running/Completed: ${pods:-ninguno}"
fi

code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 15 "http://${FQDN}/")"
if [[ "$code" != "000" ]]; then ok "HTTP público responde (${code}; sin Ingress, Traefik devuelve 404)"; else
  fail "HTTP público sin respuesta"
fi

if timeout 8 bash -c "</dev/tcp/${IP}/6443" 2>/dev/null; then fail "la API 6443 es accesible desde Internet"; else
  ok "API de Kubernetes (6443) cerrada desde Internet"
fi

out="$(ssh -o BatchMode=yes -o ConnectTimeout=20 -o StrictHostKeyChecking=yes -o UserKnownHostsFile="$KH" \
  -o PubkeyAuthentication=no -o PreferredAuthentications=password "${ADMIN}@${FQDN}" true 2>&1)"
if [[ "$out" == *"Permission denied (publickey)"* ]]; then ok "SSH solo acepta clave (sin contraseña)"; else
  fail "SSH sin clave: ${out}"
fi

echo
if ((FAILS == 0)); then
  echo "Ambiente verificado."
else
  echo "${FAILS} comprobación(es) fallaron."
  exit 1
fi
