#!/bin/bash
# DEV-10: comprueba en el clúster los controles de un ambiente y reporta todas las fallas.
# No lee el valor de ningún Secret. Crea un Pod y una NetworkPolicy temporales y los elimina al terminar.
#
# Uso: scripts/k8s-verify-security.sh <development|production>
#      KUBECTL="sudo k3s kubectl" scripts/k8s-verify-security.sh development
set -uo pipefail

case "${1:-}" in
  development | production) environment=$1 ;;
  *)
    echo "Uso: $0 <development|production>" >&2
    exit 2
    ;;
esac

namespace="eventos-$environment"
read -r -a kubectl_cmd <<<"${KUBECTL:-kubectl}"
kube() { "${kubectl_cmd[@]}" "$@"; }

# Imagen de prueba: solo se usa para intentar conexiones de red y no llega a producción.
PROBE_IMAGE=docker.io/library/busybox@sha256:a7dbc319b1438115a207e11930dc4bb3e643dcb3ec0bdd694bb36cd8fad289ec
probe_pod="netpol-probe-$$"
probe_policy="netpol-probe-egress-$$"
failures=0

check() {
  local description=$1
  shift
  if "$@" >/dev/null 2>&1; then
    echo "OK    $description"
  else
    echo "FALLA $description"
    failures=$((failures + 1))
  fi
}

not() { ! "$@"; }

as_account() {
  # auth can-i devuelve 0 si está permitido y 1 si está denegado.
  kube auth can-i "$2" "$3" -n "$namespace" --as="system:serviceaccount:$namespace:$1"
}

# Falla si algún contenedor del ambiente no cumple el contexto de seguridad esperado.
pods_are_restricted() {
  local report
  report=$(kube -n "$namespace" get pods --field-selector=status.phase=Running -o \
    jsonpath='{range .items[*]}{.metadata.name}{" "}{.spec.serviceAccountName}{" "}{.spec.automountServiceAccountToken}{" "}{.spec.securityContext.runAsNonRoot}{" "}{.spec.containers[0].securityContext.allowPrivilegeEscalation}{" "}{.spec.containers[0].securityContext.readOnlyRootFilesystem}{"\n"}{end}') || return 1
  [[ -n "$report" ]] || return 1
  ! grep -v '^[^ ]* event-service false true false true$' <<<"$report" | grep -q .
}

privileged_pod_is_rejected() {
  local output
  output=$(kube -n "$namespace" apply --dry-run=server -f - 2>&1 <<EOF
{"apiVersion": "v1", "kind": "Pod", "metadata": {"name": "privileged-probe"},
 "spec": {"containers": [{"name": "probe", "image": "$PROBE_IMAGE", "securityContext": {"privileged": true}}]}}
EOF
  )
  grep -q 'violates PodSecurity' <<<"$output"
}

secret_values_not_in_logs() {
  # Compara dentro de este proceso: los valores nunca se imprimen ni van en argumentos.
  local secret value key logs keys_template
  # shellcheck disable=SC2016 # Plantilla de kubectl, no expansión del shell.
  keys_template='go-template={{range $k, $v := .data}}{{$k}}{{"\n"}}{{end}}'
  # Incluye API, worker, PostgreSQL y los Pods de migración que aún existan.
  logs=$(kube -n "$namespace" logs -l 'app.kubernetes.io/name in (event-service,postgres)' \
    --tail=-1 --all-containers --prefix=false --max-log-requests=20 2>/dev/null) || return 1
  # Sin logs no hay nada que comparar: el control no debe pasar sin evidencia.
  [[ -n "$logs" ]] || return 1
  for secret in event-service-credentials event-service-migration postgres-credentials; do
    while read -r key; do
      value=$(kube -n "$namespace" get secret "$secret" -o "go-template={{index .data \"$key\" | base64decode}}")
      # Los nombres de usuario y de base no son secretos.
      [[ "$key" == POSTGRES_USER || "$key" == POSTGRES_DB ]] && continue
      [[ -z "$value" ]] && return 1
      [[ "$logs" == *"$value"* ]] && return 1
    done < <(kube -n "$namespace" get secret "$secret" -o "$keys_template")
  done
  return 0
}

create_probe_pod() {
  kube -n "$namespace" apply -f - <<EOF
{"apiVersion": "v1", "kind": "Pod",
 "metadata": {"name": "$probe_pod", "labels": {"app.kubernetes.io/name": "netpol-probe"}},
 "spec": {"automountServiceAccountToken": false, "restartPolicy": "Never",
  "securityContext": {"runAsNonRoot": true, "runAsUser": 65534, "seccompProfile": {"type": "RuntimeDefault"}},
  "containers": [{"name": "probe", "image": "$PROBE_IMAGE", "command": ["sleep", "300"],
   "securityContext": {"allowPrivilegeEscalation": false, "readOnlyRootFilesystem": true, "capabilities": {"drop": ["ALL"]}},
   "resources": {"requests": {"cpu": "10m", "memory": "16Mi"}, "limits": {"cpu": "100m", "memory": "32Mi"}}}]}}
EOF
  kube -n "$namespace" wait --for=condition=Ready "pod/$probe_pod" --timeout=120s
}

# Abre la salida de la sonda solo hacia la API, PostgreSQL y un puerto de control. Sin esto,
# el bloqueo se debería al egreso de la sonda y no demostraría que el ingreso rechaza la conexión.
allow_probe_egress() {
  kube -n "$namespace" apply -f - <<EOF
{"apiVersion": "networking.k8s.io/v1", "kind": "NetworkPolicy", "metadata": {"name": "$probe_policy"},
 "spec": {"podSelector": {"matchLabels": {"app.kubernetes.io/name": "netpol-probe"}}, "policyTypes": ["Egress"],
  "egress": [
   {"to": [{"podSelector": {"matchLabels": {"app.kubernetes.io/name": "postgres"}}}], "ports": [{"protocol": "TCP", "port": 5432}]},
   {"to": [{"podSelector": {"matchLabels": {"app.kubernetes.io/name": "event-service", "app.kubernetes.io/component": "api"}}}], "ports": [{"protocol": "TCP", "port": 8000}]},
   {"to": [{"namespaceSelector": {"matchLabels": {"kubernetes.io/metadata.name": "kube-system"}}, "podSelector": {"matchLabels": {"k8s-app": "kube-dns"}}}], "ports": [{"protocol": "TCP", "port": 9153}]}
  ]}}
EOF
}

remove_probe_resources() {
  kube -n "$namespace" delete networkpolicy "$probe_policy" --ignore-not-found >/dev/null 2>&1
  kube -n "$namespace" delete pod "$probe_pod" --ignore-not-found --grace-period=0 --force >/dev/null 2>&1
}
trap remove_probe_resources EXIT

# Se usan direcciones IP: un fallo de DNS no debe contarse como conexión bloqueada.
# timeout es necesario porque nc no respeta su propio límite si el paquete se descarta.
probe_reaches() {
  kube -n "$namespace" exec "$probe_pod" -- timeout 6 nc -z "$1" "$2"
}

# La política temporal tarda unos segundos en aplicarse.
probe_eventually_reaches() {
  for _ in $(seq 1 10); do
    probe_reaches "$1" "$2" && return 0
    sleep 2
  done
  return 1
}

echo "== Permisos de Kubernetes ($namespace)"
for verb_resource in "get secrets" "list pods" "create pods" "patch deployments"; do
  read -r verb resource <<<"$verb_resource"
  check "event-service no puede: $verb $resource" not as_account event-service "$verb" "$resource"
done
for verb_resource in "patch deployments" "create jobs" "get replicasets"; do
  read -r verb resource <<<"$verb_resource"
  check "event-service-deployer puede: $verb $resource" as_account event-service-deployer "$verb" "$resource"
done
for verb_resource in "get secrets" "list secrets" "create pods" "delete deployments" "create roles" "create pods/exec"; do
  read -r verb resource <<<"$verb_resource"
  check "event-service-deployer no puede: $verb $resource" not as_account event-service-deployer "$verb" "$resource"
done
check "event-service-deployer no alcanza el otro ambiente" not kube auth can-i patch deployments \
  -n "eventos-$([[ $environment == development ]] && echo production || echo development)" \
  --as="system:serviceaccount:$namespace:event-service-deployer"

echo "== Pods"
check "los Pods usan el ServiceAccount previsto, sin token, no root y con raíz de solo lectura" pods_are_restricted
check "la política restricted rechaza un Pod privilegiado" privileged_pod_is_rejected

echo "== Secretos"
check "ningún valor de los Secrets aparece en los logs de API, worker, PostgreSQL y migración" secret_values_not_in_logs

echo "== Red"
if create_probe_pod >/dev/null 2>&1; then
  postgres_ip=$(kube -n "$namespace" get pod postgres-0 -o jsonpath='{.status.podIP}' 2>/dev/null)
  api_ip=$(kube -n "$namespace" get pods -l app.kubernetes.io/component=api \
    --field-selector=status.phase=Running -o jsonpath='{.items[0].status.podIP}' 2>/dev/null)
  dns_ip=$(kube -n kube-system get service kube-dns -o jsonpath='{.spec.clusterIP}')
  check "control: la prueba sí alcanza lo permitido (DNS)" probe_reaches "$dns_ip" 53
  check "egreso: sin política temporal la sonda no abre otro puerto" not probe_reaches "$dns_ip" 9153
  check "egreso: un Pod no sale a Internet" not probe_reaches 1.1.1.1 443
  # Las pruebas de ingreso solo valen con destinos reales y con la salida de la sonda abierta.
  if [[ -z "$postgres_ip" || -z "$api_ip" ]]; then
    echo "FALLA no se obtuvo la IP de PostgreSQL o de la API: el ingreso queda sin comprobar"
    failures=$((failures + 1))
  elif allow_probe_egress >/dev/null 2>&1 && probe_eventually_reaches "$dns_ip" 9153; then
    echo "OK    control: la política temporal abre la salida de la sonda"
    check "ingreso: PostgreSQL rechaza a un Pod ajeno con salida permitida" not probe_reaches "$postgres_ip" 5432
    check "ingreso: la API rechaza a un Pod ajeno con salida permitida" not probe_reaches "$api_ip" 8000
  else
    echo "FALLA no se pudo abrir la salida de la sonda: el ingreso queda sin comprobar"
    failures=$((failures + 1))
  fi
else
  echo "FALLA no se pudo crear el Pod de prueba"
  failures=$((failures + 1))
fi

echo
if [[ "$failures" -ne 0 ]]; then
  echo "Controles con falla: $failures"
  exit 1
fi
echo "Todos los controles pasaron."
