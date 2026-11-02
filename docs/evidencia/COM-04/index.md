# COM-04 — Evidencia de validación, manejo de errores y recuperación (Escenario 3)

**Criterios:** Entradas inválidas reciben `422`, conflictos de duplicidad reciben `409`, recursos inexistentes reciben `404`, dependencias caídas reciben `503`, y el sistema recupera la operatividad sin reiniciar la aplicación. Respuestas y logs sanitizados con `request_id`. Véase [issue #4](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/4).

## 1. Pruebas automatizadas de validación y errores

```bash
uv sync --locked
uv run --locked pytest tests/test_errors.py
```

- Validación en el borde con Pydantic v2: tipos, límites de longitud, formato RFC3339 con zona horaria, tamaño de payload (16 KiB máx) y descarte de campos extra (`extra="forbid"`).
- Respuestas públicas uniformes sin filtración de trazas, contraseñas ni DSNs.
- Validación de parámetros de lote (`EventBatchCreate`) y consultas analíticas (`EventsListQuery`, `TimeseriesQuery`, `SummaryQuery`).

## 2. Experimento real de caída y recuperación (200 → 503 → 200)

Siguiendo el procedimiento documentado en [docs/arquitectura/errores.md](../../arquitectura/errores.md):

```text
status=200 request_id=before-1 code=OK
status=503 request_id=down-1 code=DEPENDENCY_UNAVAILABLE
status=200 request_id=after-1 code=OK
PASS: PostgreSQL recovered without restarting API
```

La recuperación se produce sobre el mismo PID del proceso gracias a `pool_pre_ping=True`, timeouts acotados y reintentos automáticos con backoff en lecturas operacionales.
