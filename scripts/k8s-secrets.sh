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

# Sin pipefail en sh, un fallo de od quedaría oculto y crearía credenciales vacías.
random_hex() {
  value=$(od -An -tx1 -N32 /dev/urandom | tr -d ' \n')
  if [ "${#value}" -ne 64 ]; then
    echo "No se pudo generar un valor aleatorio." >&2
    exit 1
  fi
  printf '%s' "$value"
}

found=0
for secret in $secrets; do
  # Un error de consulta o de permisos no significa que el Secret no exista.
  # shellcheck disable=SC2086 # KUBECTL puede tener varias palabras, como "sudo k3s kubectl".
  if ! existing=$($kubectl_cmd -n "$namespace" get secret "$secret" --ignore-not-found -o name); then
    echo "$namespace: no se pudo consultar $secret; no se modifica nada." >&2
    exit 1
  fi
  if [ -n "$existing" ]; then
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

# Una asignación simple propaga a set -e el fallo de $(random_hex).
admin_password=$(random_hex)
migration_password=$(random_hex)
runtime_password=$(random_hex)
producer_key=$(random_hex)
analyst_key=$(random_hex)

created=""

# Crea un Secret desde stdin y recuerda su UID. Create falla si el nombre ya existe, así que
# dos ejecuciones simultáneas no pueden crear el mismo objeto.
create_secret() {
  # shellcheck disable=SC2086
  uid=$($kubectl_cmd -n "$namespace" create -o jsonpath='{.metadata.uid}' -f -) || return 1
  [ -n "$uid" ] || return 1
  created="$created $1:$uid"
}

# Elimina solo lo creado en este intento. La precondición de UID impide borrar un reemplazo
# que otra ejecución haya creado con el mismo nombre.
abort_and_clean() {
  echo "$namespace: falló la creación; se eliminan solo los Secrets de este intento." >&2
  for item in $created; do
    name=${item%%:*}
    uid=${item#*:}
    # shellcheck disable=SC2086
    if ! printf '{"kind":"DeleteOptions","apiVersion":"v1","preconditions":{"uid":"%s"}}' "$uid" |
      $kubectl_cmd delete --raw "/api/v1/namespaces/$namespace/secrets/$name" -f - >/dev/null 2>&1; then
      echo "$namespace: no se eliminó $name: cambió o no se pudo borrar. Revisar a mano." >&2
    fi
  done
  exit 1
}

# Kubernetes no crea varios objetos en una transacción: se crean de uno en uno.
create_secret postgres-credentials <<EOF || abort_and_clean
{"apiVersion": "v1", "kind": "Secret", "metadata": {"name": "postgres-credentials"}, "type": "Opaque",
 "stringData": {"POSTGRES_USER": "bootstrap_admin", "POSTGRES_DB": "eventos", "POSTGRES_PASSWORD": "$admin_password",
                "EVENT_MIGRATION_PASSWORD": "$migration_password", "EVENT_RUNTIME_PASSWORD": "$runtime_password"}}
EOF

create_secret event-service-migration <<EOF || abort_and_clean
{"apiVersion": "v1", "kind": "Secret", "metadata": {"name": "event-service-migration"}, "type": "Opaque",
 "stringData": {"DATABASE_URL": "postgresql://eventos_migrator:$migration_password@postgres:5432/eventos"}}
EOF

create_secret event-service-credentials <<EOF || abort_and_clean
{"apiVersion": "v1", "kind": "Secret", "metadata": {"name": "event-service-credentials"}, "type": "Opaque",
 "stringData": {"DATABASE_URL": "postgresql://eventos_runtime:$runtime_password@postgres:5432/eventos",
                "EVENT_PRODUCER_KEY": "$producer_key", "EVENT_ANALYST_KEY": "$analyst_key"}}
EOF

echo "$namespace: Secrets creados."
