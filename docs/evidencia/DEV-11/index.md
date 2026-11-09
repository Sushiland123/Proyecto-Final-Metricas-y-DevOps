# DEV-11 · Salud y recursos de los workloads

[Issue #17](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/17).

## Probes

| Workload | Startup | Readiness | Liveness | Qué significa |
|---|---|---|---|---|
| API | `/health/live` | `/health/ready`, consulta la base | `/health/live` | Sin base deja de recibir tráfico, pero no se reinicia |
| Worker | `worker-health live` | `worker-health ready` | `worker-health live` | Ver abajo |
| PostgreSQL | `pg_isready` | `pg_isready` | `pg_isready`, 6 fallas | Da margen a recuperaciones lentas |
| Job de migración | No aplica | No aplica | No aplica | Proceso finito, `activeDeadlineSeconds` y sin reintentos |

El worker no tiene HTTP y la imagen no tiene shell, así que usa un subcomando del binario. El bucle escribe dos heartbeats en un `emptyDir` en memoria:

- `live`, en cada vuelta. Falla si el bucle se traba por más de 120 s. Ese límite supera la espera máxima de reintento (60 s), para que una caída de la base no reinicie el worker.
- `ready`, solo tras una consulta correcta. Falla si pasan más de 30 s sin una. Así un worker con credenciales malas no completa el rollout.

## Recursos

| Workload | Requests | Limits | Medido con carga |
|---|---|---|---|
| API | 100m / 128Mi | 500m / 256Mi | 201m / 8Mi |
| Worker | 100m / 128Mi | 500m / 256Mi | 47m / 3Mi |
| PostgreSQL | 250m / 256Mi | 1 CPU / 1Gi | 311m / 42Mi |

La memoria medida está muy por debajo de lo solicitado, y la CPU de PostgreSQL superó su request sin llegar al límite. Los requests reservan capacidad en el nodo, así que sobredimensionarlos no cuesta dinero en una VM de precio fijo, pero sí reduce lo que queda para la observabilidad. No se ajustan todavía: esta medición viene de un equipo distinto a la VM.

Superar el límite de memoria provoca `OOMKilled` y un reinicio. Superar el de CPU solo limita el proceso.

## Resultado obtenido · 2026-11-09, ensayo local

**No es la evidencia final.** K3s `v1.35.2+k3s1` desechable en Docker, con una imagen construida desde este commit.

| Prueba | Resultado |
|---|---|
| PostgreSQL detenido durante 150 s | API y worker dejan de estar listos, el endpoint queda sin tráfico y hay 0 reinicios |
| PostgreSQL restaurado | Ambos vuelven a estar listos sin reiniciarse |
| Liveness del worker forzada a fallar | `Container worker failed liveness probe, will be restarted`, 2 reinicios |
| Imagen sin `worker-health` | El rollout falla y no se promueve |
| Carga de 90 s, unas 126 lecturas por segundo | Consumo de la tabla anterior |

Una imagen construida desde Windows puede traer los `.sql` con CRLF, y SQLx rechazaría una base migrada con LF. La imagen publicada por CI no tiene este problema.

## Pendiente

- [ ] Repetir las pruebas y la medición en Azure, y ajustar requests y limits con esos datos.
- [ ] Enlazar la evidencia. La imagen con `worker-health` solo existe después de integrar este PR.
