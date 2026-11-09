#!/bin/bash
# Escribe en stdout un script autocontenido que ejecuta una entrega dentro de la VM.
# Azure Run Command solo recibe texto, así que los manifiestos y scripts van embebidos.
# El bundle no contiene secretos: se leen en el clúster.
#
# Uso: scripts/k8s-bundle.sh <development|production> <imagen@sha256:...>
set -euo pipefail

environment=${1:-}
image=${2:-}
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
postgres_image=docker.io/library/postgres@sha256:5264710be20f409750ac6dc4e5706dcef929c87e241605463f0f9aefa932a8d9
build=$(mktemp -d)
trap 'rm -rf "$build"' EXIT

for stage in migration workloads; do
  python3 "$root/scripts/render-k8s.py" --environment "$environment" --stage "$stage" \
    --image "$image" --postgres-image "$postgres_image" --output "$build/$stage.json"
done

embed() {
  echo "base64 -d >\"\$work/$2\" <<'BASE64'"
  base64 "$1"
  echo BASE64
}

cat <<EOF
#!/bin/bash
set -euo pipefail
export KUBECTL="\${KUBECTL:-k3s kubectl}"
work=\$(mktemp -d)
trap 'rm -rf "\$work"' EXIT
EOF
embed "$root/scripts/k8s-deliver.sh" k8s-deliver.sh
embed "$root/scripts/k8s-smoke.sh" k8s-smoke.sh
embed "$build/migration.json" migration.json
embed "$build/workloads.json" workloads.json
# render-k8s.py ya validó el ambiente y el digest.
# shellcheck disable=SC2016 # $work se expande en la VM, no aquí.
printf 'bash "$work/k8s-deliver.sh" %q %q "$work"\n' "$environment" "$image"
