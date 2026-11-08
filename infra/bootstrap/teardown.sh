#!/usr/bin/env bash
# Deshace el bootstrap (DEV-13): asignaciones de rol de las identidades en la
# suscripción, el rol personalizado y el Resource Group del state (Storage,
# identidades y credenciales federadas). Borrar solo el Resource Group dejaría
# asignaciones huérfanas y el rol personalizado en la suscripción.
#
# Ejecutar SOLO después de `terraform destroy`: elimina el state. Pide escribir
# el sufijo para confirmar.
set -euo pipefail
export MSYS_NO_PATHCONV=1 # Git Bash en Windows: no convertir /subscriptions/... en rutas.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENV_FILE="$ROOT/infra/bootstrap/bootstrap.env"
[[ -f "$ENV_FILE" ]] || { echo "No existe $ENV_FILE: no hay bootstrap que deshacer." >&2; exit 1; }
# shellcheck disable=SC1090
source "$ENV_FILE"
: "${NAME_SUFFIX:?bootstrap.env sin NAME_SUFFIX}"

RG="${TFSTATE_RESOURCE_GROUP:-rg-eventos-tfstate}"
ROLE="eventos-${NAME_SUFFIX}-vm-operator"
SUB_SCOPE="/subscriptions/$(az account show --query id -o tsv | tr -d '\r')"

if [[ "$(az group exists --name "rg-eventos-${NAME_SUFFIX}" | tr -d '\r')" == "true" ]]; then
  echo "rg-eventos-${NAME_SUFFIX} todavía existe: ejecutar primero terraform destroy." >&2
  exit 1
fi

echo "Se eliminarán en ${SUB_SCOPE}:"
echo "  - las asignaciones de rol de id-eventos-plan, id-eventos-apply e id-eventos-ops;"
echo "  - el rol personalizado ${ROLE};"
echo "  - el Resource Group ${RG} (state ${TFSTATE_STORAGE_ACCOUNT:-?}, identidades y credenciales federadas)."
read -r -p "Escribir el sufijo (${NAME_SUFFIX}) para confirmar: " answer
[[ "$answer" == "$NAME_SUFFIX" ]] || { echo "Cancelado."; exit 1; }

for id in id-eventos-plan id-eventos-apply id-eventos-ops; do
  pid="$(az identity show -g "$RG" -n "$id" --query principalId -o tsv 2>/dev/null | tr -d '\r' || true)"
  if [[ -n "$pid" ]]; then
    az role assignment delete --assignee-object-id "$pid" --scope "$SUB_SCOPE" -o none
    echo "Asignaciones de ${id} en la suscripción eliminadas."
  fi
done

if [[ "$(az role definition list --custom-role-only true --name "$ROLE" --query 'length(@)' -o tsv | tr -d '\r')" != "0" ]]; then
  az role definition delete --name "$ROLE" --custom-role-only true --scope "$SUB_SCOPE" -o none
  echo "Rol ${ROLE} eliminado."
fi

# Las asignaciones con alcance en el Storage se eliminan junto con el recurso.
az group delete --name "$RG" --yes -o none
echo "Resource Group ${RG} eliminado."
echo "Comprobar: az group exists --name ${RG}   (esperado: false)"
