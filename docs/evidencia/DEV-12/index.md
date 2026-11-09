# DEV-12 · Recuperar una versión funcional

[Issue #18](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/18).

## Runbook

1. **Detectar.** La entrega falla en el rollout o en el smoke, o una probe saca el Pod del servicio.
2. **Diagnosticar**, sin cambiar nada:

   ```bash
   sudo k3s kubectl -n eventos-production get pods
   sudo k3s kubectl -n eventos-production describe deployment event-service-api
   sudo k3s kubectl -n eventos-production get events --sort-by=.lastTimestamp
   sudo k3s kubectl -n eventos-production logs deployment/event-service-api --tail=50
   ```

3. **Revertir.** `KUBECTL="sudo k3s kubectl" bash scripts/k8s-rollback.sh production`. Vuelve API y worker a la revisión anterior y ejecuta el smoke.
4. **Verificar.** El script termina con `ROLLBACK_OK` y el digest activo. Confirmar los Pods con `get pods`.
5. **Registrar** causa, digest anterior, digest restaurado y tiempo.

`k8s-deliver.sh` hace los pasos 3 y 4 solo cuando falla una entrega.

## Qué no hace un rollback

- **No restaura datos.** Solo cambia los Deployments; el PVC de PostgreSQL no se toca.
- **No revierte migraciones.** No hay downgrades automáticos. Toda migración debe ser compatible con la versión anterior mientras dure una entrega: agregar columnas o tablas, no borrarlas ni renombrarlas en el mismo cambio.
- **Si la base se dañó,** el rollback no alcanza: hace falta un respaldo, y hoy no hay uno automatizado.

## Simulacro

[`k8s-rollback-drill.sh`](../../../scripts/k8s-rollback-drill.sh) solo actúa sobre development. Provoca una imagen inexistente y luego una configuración inválida, recupera y compara el número de lecturas.

## Resultado obtenido · 2026-11-09, ensayo local

**No es la evidencia final.** K3s `v1.35.2+k3s1` desechable en Docker.

| Falla provocada | Cómo se detectó | Tiempo total | Solo rollback y smoke | Después |
|---|---|---|---|---|
| Imagen inexistente | `ImagePullBackOff`, el rollout no termina | 84 s | 7 s | Versión anterior activa y smoke correcto |
| `BIND_ADDR` inválido | `CrashLoopBackOff`, el rollout no termina | 83 s | 6 s | Versión anterior activa y smoke correcto |

El tiempo total incluye 75 s de espera antes de declarar fallido el rollout. Las lecturas pasaron de 1 a 3: se conservaron las anteriores y se sumaron las de los smoke. El Pod anterior siguió atendiendo durante las dos fallas, porque `maxUnavailable` es 0.

## Pendiente

- [ ] Ejecutar el simulacro en development en Azure y enlazar la salida.
