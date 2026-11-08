#!/usr/bin/env bash
# Inicialización única de la infraestructura (DEV-07).
#
# Crea fuera de Terraform lo que Terraform necesita para existir:
#   - Storage Account del state remoto, solo con autenticación Entra ID,
#     versionado de blobs y retención de borrado;
#   - identidades de GitHub Actions: plan (pull requests) y apply (environment azure-infra);
#   - credenciales federadas OIDC, sin client secrets;
#   - RBAC mínimo de cada identidad.
#
# Es idempotente: reutiliza el sufijo guardado en infra/bootstrap/bootstrap.env.
# Requiere Azure CLI con sesión de un usuario Owner de la suscripción, GitHub CLI y openssl.
set -euo pipefail
export MSYS_NO_PATHCONV=1 # Git Bash en Windows: no convertir /subscriptions/... en rutas.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENV_FILE="$ROOT/infra/bootstrap/bootstrap.env"

LOCATION="${LOCATION:-westus}"
REPO="${GITHUB_REPOSITORY:-marchelo23/Proyecto-Final-Metricas-y-DevOps}"
APPLY_ENVIRONMENT="azure-infra"

if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  source "$ENV_FILE"
fi
SUFFIX="${NAME_SUFFIX:-$(openssl rand -hex 3)}"

RG="rg-eventos-tfstate"
STORAGE="sttfeventos${SUFFIX}"
CONTAINER="tfstate"
PLAN_ID="id-eventos-plan"
APPLY_ID="id-eventos-apply"

SUBSCRIPTION_ID="$(az account show --query id -o tsv)"
TENANT_ID="$(az account show --query tenantId -o tsv)"
CURRENT_USER_ID="$(az ad signed-in-user show --query id -o tsv)"
SUB_SCOPE="/subscriptions/${SUBSCRIPTION_ID}"

ensure_role() {
  local principal="$1" type="$2" role="$3" scope="$4"
  local count
  count="$(az role assignment list --scope "$scope" \
    --query "[?principalId=='${principal}' && roleDefinitionName=='${role}'] | length(@)" -o tsv)"
  if [[ "$count" == "0" ]]; then
    az role assignment create --assignee-object-id "$principal" --assignee-principal-type "$type" \
      --role "$role" --scope "$scope" -o none
  fi
}

ensure_fic() {
  local identity="$1" name="$2" subject="$3"
  if ! az identity federated-credential show -g "$RG" --identity-name "$identity" -n "$name" -o none 2>/dev/null; then
    az identity federated-credential create -g "$RG" --identity-name "$identity" -n "$name" \
      --issuer "https://token.actions.githubusercontent.com" \
      --subject "$subject" --audiences "api://AzureADTokenExchange" -o none
  fi
}

echo "== Resource providers"
for ns in Microsoft.Storage Microsoft.ManagedIdentity Microsoft.Compute Microsoft.Network Microsoft.DevTestLab; do
  az provider register --namespace "$ns" --wait -o none
done

echo "== Backend del state: ${STORAGE}/${CONTAINER}"
az group create --name "$RG" --location "$LOCATION" --tags project=eventos purpose=tfstate -o none
if ! az storage account show -g "$RG" -n "$STORAGE" -o none 2>/dev/null; then
  az storage account create --name "$STORAGE" --resource-group "$RG" --location "$LOCATION" \
    --sku Standard_LRS --kind StorageV2 --https-only true --min-tls-version TLS1_2 \
    --allow-blob-public-access false --allow-shared-key-access false -o none
fi
az storage account blob-service-properties update --account-name "$STORAGE" --resource-group "$RG" \
  --enable-versioning true --enable-delete-retention true --delete-retention-days 14 \
  --enable-container-delete-retention true --container-delete-retention-days 14 -o none
STORAGE_ID="$(az storage account show -g "$RG" -n "$STORAGE" --query id -o tsv)"

# Sin access keys: el usuario necesita un rol de datos para crear el container.
ensure_role "$CURRENT_USER_ID" User "Storage Blob Data Contributor" "$STORAGE_ID"
for attempt in $(seq 1 18); do
  if az storage container create --name "$CONTAINER" --account-name "$STORAGE" --auth-mode login -o none 2>/dev/null; then
    break
  fi
  [[ "$attempt" == 18 ]] && { echo "No se pudo crear el container (propagación de RBAC)." >&2; exit 1; }
  sleep 10
done

echo "== Identidades de GitHub Actions"
az identity create -g "$RG" -n "$PLAN_ID" --location "$LOCATION" -o none
az identity create -g "$RG" -n "$APPLY_ID" --location "$LOCATION" -o none
PLAN_CLIENT_ID="$(az identity show -g "$RG" -n "$PLAN_ID" --query clientId -o tsv)"
PLAN_PRINCIPAL_ID="$(az identity show -g "$RG" -n "$PLAN_ID" --query principalId -o tsv)"
APPLY_CLIENT_ID="$(az identity show -g "$RG" -n "$APPLY_ID" --query clientId -o tsv)"
APPLY_PRINCIPAL_ID="$(az identity show -g "$RG" -n "$APPLY_ID" --query principalId -o tsv)"

ensure_role "$PLAN_PRINCIPAL_ID" ServicePrincipal "Reader" "$SUB_SCOPE"
ensure_role "$PLAN_PRINCIPAL_ID" ServicePrincipal "Storage Blob Data Contributor" "$STORAGE_ID"
ensure_role "$APPLY_PRINCIPAL_ID" ServicePrincipal "Contributor" "$SUB_SCOPE"
ensure_role "$APPLY_PRINCIPAL_ID" ServicePrincipal "Storage Blob Data Contributor" "$STORAGE_ID"

echo "== Credenciales federadas para ${REPO}"
OWNER="${REPO%%/*}"
NAME="${REPO#*/}"
OWNER_ID="$(gh api "users/${OWNER}" --jq .id)"
REPO_ID="$(gh api "repos/${REPO}" --jq .id)"
# GitHub puede emitir el subject con o sin los IDs inmutables; se registran ambos formatos.
for prefix in "repo:${OWNER}/${NAME}" "repo:${OWNER}@${OWNER_ID}/${NAME}@${REPO_ID}"; do
  tag="$([[ "$prefix" == *@* ]] && echo ids || echo nombre)"
  ensure_fic "$PLAN_ID" "github-pr-${tag}" "${prefix}:pull_request"
  ensure_fic "$APPLY_ID" "github-${APPLY_ENVIRONMENT}-${tag}" "${prefix}:environment:${APPLY_ENVIRONMENT}"
done

cat > "$ENV_FILE" <<ENV
NAME_SUFFIX=${SUFFIX}
LOCATION=${LOCATION}
AZURE_SUBSCRIPTION_ID=${SUBSCRIPTION_ID}
AZURE_TENANT_ID=${TENANT_ID}
TFSTATE_RESOURCE_GROUP=${RG}
TFSTATE_STORAGE_ACCOUNT=${STORAGE}
TFSTATE_CONTAINER=${CONTAINER}
PLAN_CLIENT_ID=${PLAN_CLIENT_ID}
APPLY_CLIENT_ID=${APPLY_CLIENT_ID}
ENV

cat <<MSG

Bootstrap completo. Valores guardados en infra/bootstrap/bootstrap.env (no versionado).

1. Verificar que infra/terraform/backend.hcl use storage_account_name = "${STORAGE}".
2. Un administrador del repositorio ${REPO} debe crear el environment
   "${APPLY_ENVIRONMENT}" (con revisores requeridos) y estas variables de GitHub Actions:
     AZURE_SUBSCRIPTION_ID=${SUBSCRIPTION_ID}
     AZURE_TENANT_ID=${TENANT_ID}
     PLAN_CLIENT_ID=${PLAN_CLIENT_ID}
     APPLY_CLIENT_ID=${APPLY_CLIENT_ID}
     INFRA_NAME_SUFFIX=${SUFFIX}
     INFRA_LOCATION=${LOCATION}
     INFRA_ADMIN_SSH_PUBLIC_KEY=<clave pública SSH>
     INFRA_ADMIN_CIDRS=["<IP pública>/32"]
     INFRA_BUDGET_CONTACT_EMAILS=["<correo>"]
MSG
