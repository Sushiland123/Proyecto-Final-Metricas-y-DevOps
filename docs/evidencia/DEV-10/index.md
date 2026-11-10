# DEV-10 · Configuración, secretos y privilegio mínimo

[Issue #16](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/16).

## Cómo está separado

| Qué | Dónde vive | Cómo llega al Pod |
|---|---|---|
| Configuración no secreta | `ConfigMap/event-service-config` | `envFrom` |
| Contraseñas, DSN y claves de API | Secrets creados en el clúster con [`k8s-secrets.sh`](../../../scripts/k8s-secrets.sh) | `envFrom` o `secretKeyRef` |
| Binario | Imagen por digest, sin configuración de ambiente | — |

Cada proceso recibe solo lo que usa: el worker no recibe las claves de API, el Job de migración usa otra identidad de base de datos y `bootstrap_admin` solo llega a PostgreSQL.

## Privilegio mínimo

- **Pods:** `ServiceAccount/event-service`, sin token montado y sin permisos. Namespace con Pod Security `restricted`, usuario no root, raíz de solo lectura y sin capabilities.
- **Despliegue:** `ServiceAccount/event-service-deployer`, limitado a su namespace. Actualiza Deployments y crea Jobs, pero no accede a la API de Secrets, no crea Pods directamente y no modifica permisos.
- **Red:** todo denegado por defecto, con dos excepciones: DNS y `event-service → postgres:5432`. PostgreSQL no se publica.

## Qué protege un Secret y qué no

- **Protege** de que el valor entre a Git o a la imagen, y limita por RBAC quién lo lee por la API.
- **No cifra:** los valores van en base64. K3s no tiene activado `--secrets-encryption`, así que están sin cifrar en el datastore de la VM. Activarlo cambia cloud-init, lo que reemplazaría la VM y los volúmenes.
- **Quién puede leerlos:** cualquiera con `cluster-admin` o root en la VM, incluidos quienes entran por SSH como `azureuser`, que tiene `sudo`. También cualquier proceso dentro de un Pod que los reciba.
- **Rotación:** cambiar un Secret no cambia la contraseña en una base existente, y cada contraseña de rol está en dos Secrets:

  | Rol | En `postgres-credentials` | En el DSN de |
  |---|---|---|
  | `eventos_runtime` | `EVENT_RUNTIME_PASSWORD` | `event-service-credentials` |
  | `eventos_migrator` | `EVENT_MIGRATION_PASSWORD` | `event-service-migration` |

  Hay que ejecutar `ALTER ROLE`, actualizar los dos Secrets y reiniciar los Deployments, porque la configuración se lee al iniciar. Entre `ALTER ROLE` y el reinicio, las conexiones nuevas fallan: hay una interrupción breve. Si se actualiza solo uno, un volumen inicializado después crearía roles con contraseñas distintas a las de los clientes.

## Límite de la cuenta de despliegue

Esta cuenta no puede pedir un Secret a la API, pero tiene un acceso indirecto: podría crear un Job que monte un Secret y lo escriba en sus logs, y luego leerlos con `pods/log`. Pod Security `restricted` no lo impide, y bloquearlo exigiría un control de admisión que este proyecto no tiene. Por eso la cuenta debe tratarse como de confianza alta. El diseño es que solo la use el pipeline desde `main`, con código revisado, pero ese control todavía no rige: el workflow llega con DEV-05 y, hasta ahora, los scripts los ejecutaron por SSH administradores que ya tienen `sudo`. Su valor real es acotar los errores del despliegue a un namespace.

## Comprobación

[`k8s-verify-security.sh`](../../../scripts/k8s-verify-security.sh) comprueba permisos con `kubectl auth can-i`, contexto de los Pods, rechazo de un Pod privilegiado, ausencia de valores de Secrets en logs y aislamiento de red. Compara los valores dentro del proceso sin imprimirlos. Revisa los logs disponibles de API, worker, PostgreSQL y los Pods de migración, que se eliminan a las 24 h de terminar el Job.

[`k8s-secrets.sh`](../../../scripts/k8s-secrets.sh) elimina lo creado en su propio intento si falla a medias, para no dejar un conjunto parcial con contraseñas perdidas.

Para el ingreso, la sonda recibe durante la prueba una NetworkPolicy temporal que le abre la salida solo hacia la API, PostgreSQL y un puerto de control. Sin ella, la conexión se cortaría por el egreso de la propia sonda y la prueba no demostraría nada sobre el ingreso. El puerto de control confirma que la política temporal ya rige antes de evaluar el rechazo. Al terminar se eliminan el Pod y la política.

## Resultado obtenido · 2026-11-09, ensayo local

**No es la evidencia final.** Se ejecutó en un K3s `v1.35.2+k3s1` desechable en Docker, con la imagen publicada `sha256:bd50b3cf…`.

| Prueba | Resultado |
|---|---|
| `k8s-secrets.sh`, primera ejecución y repetición | Crea los tres; la repetición no modifica nada |
| Anotaciones del Secret | Sin `last-applied-configuration` |
| `k8s-verify-security.sh development` | 21 de 21 controles correctos |
| Control negativo, sin ninguna NetworkPolicy | 3 fallas de red detectadas, código de salida 1 |
| Políticas restauradas | Vuelve a pasar |
| `PROCESSOR_POLL_MS` cambiado y reinicio del worker | Mismo digest: la configuración cambia sin reconstruir la imagen |

## Resultado en Azure · 2026-11-09

Clúster K3s `v1.35.2+k3s1` de la VM del proyecto, namespace `eventos-development`, commit `ad1c0c6`, [ejecución de publicación](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/actions/runs/42542020614), digest `sha256:bd50b3cf03f23eb754c3635110222330ac6cba76b404101020eb091ef2845095`.

| Prueba | Resultado |
|---|---|
| `k8s-secrets.sh` en los dos ambientes | Secrets creados sin mostrar valores |
| `k8s-verify-security.sh development` | 21 de 21 controles correctos |
| `PROCESSOR_POLL_MS` cambiado y reinicio del worker | Digest `bd50b3cf03f2` antes y después |
| Desde Internet | 6443, 5432 y 8000 no responden |

### Prueba de ingreso con salida permitida

La tabla anterior corresponde a la primera versión del script. La versión corregida se ejecutó en el mismo clúster:

| Prueba | Resultado |
|---|---|
| `k8s-verify-security.sh development` con las políticas normales | 23 de 23 controles correctos |
| Control negativo: ingreso abierto y egreso denegado, script anterior | Todos los controles pasan: falso positivo |
| Control negativo: ingreso abierto y egreso denegado, script corregido | 2 fallas, PostgreSQL y API, código de salida 1 |
| Políticas restauradas | 23 de 23; sin Pods ni políticas temporales restantes |
| API sin Pods | El control falla: no se da por bueno el ingreso sin una IP real |
| Logs revisados | API, worker, PostgreSQL y 2 Pods de migración, sin valores de Secrets |

El control negativo quitó `Ingress` de `default-deny` y eliminó `postgres-from-event-service`. Se restauró aplicando el manifiesto `foundation`.

La misma comprobación debe repetirse en production cuando tenga workloads. Sin Pods de aplicación, el control falla por falta de evidencia.
