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
- **Despliegue:** `ServiceAccount/event-service-deployer`, limitado a su namespace. Actualiza Deployments y crea Jobs, pero no lee Secrets, no crea Pods y no modifica permisos.
- **Red:** todo denegado por defecto, con dos excepciones: DNS y `event-service → postgres:5432`. PostgreSQL no se publica.

## Qué protege un Secret y qué no

- **Protege** de que el valor entre a Git o a la imagen, y limita por RBAC quién lo lee por la API.
- **No cifra:** los valores van en base64. K3s no tiene activado `--secrets-encryption`, así que están sin cifrar en el datastore de la VM. Activarlo cambia cloud-init, lo que reemplazaría la VM y los volúmenes.
- **Quién puede leerlos:** cualquiera con `cluster-admin` o root en la VM, incluidos quienes entran por SSH como `azureuser`, que tiene `sudo`. También cualquier proceso dentro de un Pod que los reciba.
- **Rotación:** cambiar el Secret no cambia la contraseña en una base existente. Hay que ejecutar `ALTER ROLE` con la nueva contraseña, actualizar el Secret y reiniciar los Deployments, porque la configuración se lee al iniciar.

## Comprobación

[`k8s-verify-security.sh`](../../../scripts/k8s-verify-security.sh) comprueba permisos con `kubectl auth can-i`, contexto de los Pods, rechazo de un Pod privilegiado, ausencia de valores de Secrets en logs y aislamiento de red. Compara los valores dentro del proceso sin imprimirlos.

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

## Pendiente

- [ ] Ejecutar `k8s-verify-security.sh` en los dos namespaces del clúster de Azure y enlazar el resultado.
