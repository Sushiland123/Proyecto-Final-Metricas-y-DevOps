# DEV-09 · Workloads para K3s

[Issue #15](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/15). Base Azure/K3s entregada en PR #40; manifiestos y procedimiento de aplicación: [kubernetes/README.md](../../../kubernetes/README.md).

## Verificación local · 2026-11-09 UTC

- Siete pruebas de contratos offline: separación API/worker/migración, namespaces distintos, digest obligatorio, Service solo para API, PVC retenido, límites y contexto non-root, sin tokens montados y probes HTTP solo en la API.
- kubeconform `v0.8.0` con checksum verificado, esquemas Kubernetes `1.35.0`, modo estricto: **20 recursos válidos, 0 inválidos, 0 errores, 0 omitidos** (10 por ambiente, tres etapas). Los digests de esas pruebas son ficticios, solo validan esquema y nunca se aplican.
- `bash -n kubernetes/postgres-init.sh`: correcto.
- PostgreSQL **16.15 bookworm**, digest amd64 `sha256:5264710be20f409750ac6dc4e5706dcef929c87e241605463f0f9aefa932a8d9`, probado en un contenedor local propio, red deshabilitada, UID 999, raíz de solo lectura y mounts escribibles para data/run/tmp. Init correcto, rol migrator con DDL; runtime con SELECT/INSERT/UPDATE y rechazo de CREATE TABLE/DELETE. Se eliminaron exclusivamente el contenedor/volumen aleatorios creados por la prueba.
- Comandos reproducibles: `python3 -m unittest discover -s scripts/tests -p test_k8s_render.py -v` y `python3 scripts/test-postgres-init.py` (este último requiere Docker y crea/elimina recursos locales propios; no contacta Azure).

## Pendiente para cerrar #15

- [ ] CI del último commit y revisión del PR.
- [ ] Dry-run de servidor y despliegue autorizado en el K3s de Azure.
- [ ] Nodo `Ready`, Pods saludables, versión K3s y NSG verificados en el ambiente.
- [ ] Job de migración exitoso con imagen por digest y smoke que acredite API y procesamiento real.
- [ ] Prueba de reinicio de Pod con conservación de datos y evidencia publicada sin secretos.

No se invocó `kubectl`, Terraform ni operaciones de Azure. Validación de esquema y prueba local de PostgreSQL **no equivalen** a despliegue, RBAC de Kubernetes probado, capacidad medida, recuperación de VM o defensa académica.
