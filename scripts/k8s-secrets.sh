#!/bin/sh
# DEV-10: crea en el clúster los Secrets de un ambiente, con valores aleatorios propios.
#
# Los valores no pasan por argumentos, archivos ni la salida del comando: se generan
# aquí y llegan a kubectl por stdin. Se usa `create` y no `apply`, porque apply guardaría
# el Secret completo en la anotación last-applied.
#
# Uso: scripts/k8s-secrets.sh <development|production>
#      KUBECTL="sudo k3s kubectl" scripts/k8s-secrets.sh development
set -eu

case "${1:-}" in
  development | production) environment=$1 ;;
  *)
    echo "Uso: $0 <development|production>" >&2
    exit 2
    ;;
esac

namespace="eventos-$environment"
kubectl_cmd=${KUBECTL:-kubectl}
secrets="postgres-credentials event-service-migration event-service-credentials"

random_hex() {
  od -An -tx1 -N32 /dev/urandom | tr -d ' \n'
}

found=0
for secret in $secrets; do
  # shellcheck disable=SC2086 # KUBECTL puede tener varias palabras, como "sudo k3s kubectl".
  if $kubectl_cmd -n "$namespace" get secret "$secret" >/dev/null 2>&1; then
    found=$((found + 1))
  fi
done

if [ "$found" -eq 3 ]; then
  echo "$namespace: los Secrets ya existen; no se modifican."
  exit 0
fi
if [ "$found" -ne 0 ]; then
  # Regenerar solo una parte dejaría contraseñas distintas a las de los roles de la base.
  echo "$namespace: existen $found de 3 Secrets. Revisar a mano; no se modifica nada." >&2
  exit 1
fi

admin_password=$(random_hex)
migration_password=$(random_hex)
runtime_password=$(random_hex)
producer_key=$(random_hex)
analyst_key=$(random_hex)

# shellcheck disable=SC2086
$kubectl_cmd -n "$namespace" create -f - >/dev/null <<EOF
{"apiVersion": "v1", "kind": "List", "items": [
  {"apiVersion": "v1", "kind": "Secret", "metadata": {"name": "postgres-credentials"}, "type": "Opaque",
   "stringData": {"POSTGRES_USER": "bootstrap_admin", "POSTGRES_DB": "eventos", "POSTGRES_PASSWORD": "$admin_password",
                  "EVENT_MIGRATION_PASSWORD": "$migration_password", "EVENT_RUNTIME_PASSWORD": "$runtime_password"}},
  {"apiVersion": "v1", "kind": "Secret", "metadata": {"name": "event-service-migration"}, "type": "Opaque",
   "stringData": {"DATABASE_URL": "postgresql://eventos_migrator:$migration_password@postgres:5432/eventos"}},
  {"apiVersion": "v1", "kind": "Secret", "metadata": {"name": "event-service-credentials"}, "type": "Opaque",
   "stringData": {"DATABASE_URL": "postgresql://eventos_runtime:$runtime_password@postgres:5432/eventos",
                  "EVENT_PRODUCER_KEY": "$producer_key", "EVENT_ANALYST_KEY": "$analyst_key"}}
]}
EOF

echo "$namespace: Secrets creados."
