# DEV-09 · Workloads para K3s

[Issue #15](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/15). Base Azure/K3s entregada en PR #40; manifiestos y procedimiento de aplicación: [kubernetes/README.md](../../../kubernetes/README.md).

## Verificación local · 2026-11-09 UTC

- Siete pruebas de contratos offline: separación API/worker/migración, namespaces distintos, digest obligatorio, Service solo para API, PVC retenido, límites y contexto non-root, sin tokens montados y probes HTTP solo en la API.
- kubeconform `v0.8.0` con checksum verificado, esquemas Kubernetes `1.35.0`, modo estricto: **20 recursos válidos, 0 inválidos, 0 errores, 0 omitidos** (10 por ambiente, tres etapas). Los digests de esas pruebas son ficticios, solo validan esquema y nunca se aplican.
- `bash -n kubernetes/postgres-init.sh`: correcto.
- PostgreSQL **16.15 bookworm**, digest amd64 `sha256:5264710be20f409750ac6dc4e5706dcef929c87e241605463f0f9aefa932a8d9`, probado en un contenedor local propio, red deshabilitada, UID 999, raíz de solo lectura y mounts escribibles para data/run/tmp. Init correcto, rol migrator con DDL; runtime con SELECT/INSERT/UPDATE y rechazo de CREATE TABLE/DELETE. Se eliminaron exclusivamente el contenedor/volumen aleatorios creados por la prueba.
- Comandos reproducibles: `python3 -m unittest discover -s scripts/tests -p test_k8s_render.py -v` y `python3 scripts/test-postgres-init.py` (este último requiere Docker y crea/elimina recursos locales propios; no contacta Azure).

## Resultado en Azure · 2026-11-09

Desplegado en la VM del proyecto por SSH con los scripts del PR #62 y siguientes. Imagen: commit `ad1c0c6`, [ejecución de publicación](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/actions/runs/42542020614), digest `sha256:bd50b3cf03f23eb754c3635110222330ac6cba76b404101020eb091ef2845095`. La firma se comprobó con `gh attestation verify`.

| Criterio | Resultado |
|---|---|
| Nodo | `k3s-eventos`, `Ready`, K3s `v1.35.2+k3s1`, Ubuntu 24.04 |
| Workloads en `eventos-development` | PostgreSQL, API y worker `1/1 Running`, 0 reinicios |
| Job de migración | `Completed` con el mismo digest |
| Prueba funcional | Lectura enviada por la API y marcada como procesada por el worker |
| Digest en ejecución | `imageID` de API, worker y migración: `bd50b3cf03f2` |
| Reinicio de PostgreSQL y API | Lecturas antes 1, después 1; smoke correcto |
| Puertos desde Internet | 22 solo para IP administradas; 80 y 443 abiertos; 6443, 5432 y 8000 sin respuesta |

Un nodo es una decisión aceptada por el enunciado: si se pierde la VM o su disco, se pierden los volúmenes `local-path`.

## Pendiente para cerrar #15

- [x] CI del último commit y revisión del PR.
- [x] Dry-run de servidor y despliegue autorizado en el K3s de Azure.
- [x] Nodo `Ready`, Pods saludables, versión K3s y NSG verificados en el ambiente.
- [x] Job de migración exitoso con imagen por digest y smoke que acredite API y procesamiento real.
- [x] Prueba de reinicio de Pod con conservación de datos y evidencia publicada sin secretos.

La sección de verificación local no usó `kubectl`, Terraform ni Azure. La recuperación ante la pérdida de la VM y la defensa académica no se acreditan aquí.
