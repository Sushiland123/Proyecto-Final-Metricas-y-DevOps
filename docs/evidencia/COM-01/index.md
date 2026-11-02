# COM-01 — Evidencia de funciones y reglas del Escenario 3

**Criterio:** Las funciones y reglas definidas para el [Escenario 3: Ingesta y Análisis de Eventos](../../escenario.md) cuentan con pruebas de aceptación ejecutables, automatizadas y trazables regla por regla. Véase [issue #1](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/1).

## 1. Reproducción contra base PostgreSQL aislada

Con `TEST_DATABASE_URL=DATABASE_URL` apuntando exclusivamente a una base de prueba (`events_test`), claves de prueba configuradas en el entorno y PostgreSQL 16 activo:

```bash
uv sync --locked
uv run --locked alembic upgrade head
uv run --locked pytest tests/test_acceptance.py --junitxml=artifacts/junit.xml --html=artifacts/acceptance.html --self-contained-html
```

**Resultado esperado:**
- Ingesta puntual (`POST /v1/events`) y en lote (`POST /v1/events/batch`) con confirmación transaccional y rollback atómico ante duplicados.
- Deduplicación estricta por `(source, external_id)` con código `409 Conflict`.
- Consultas puntuales por UUID (`GET /v1/events/{id}`) y listados paginados con filtros (`GET /v1/events`).
- Agregación dimensional (`GET /v1/analysis/counts`), series temporales con intervalos de truncado (`GET /v1/analysis/timeseries`) y resúmenes estadísticos numéricos (`GET /v1/analysis/summary`).
- Control de acceso RBAC por ruta y servicio, y aislamiento lateral por origen emisor.
- Manejo uniforme de entradas inválidas (`422 Unprocessable Entity`) y sondas públicas (`/health/live`, `/health/ready`).

## 2. Trazabilidad de reglas (Escenario 3 → Suite de Aceptación)

La matriz detallada en [docs/escenario.md](../../escenario.md) conecta explícitamente cada regla (E3-01 a E3-16) con su caso de prueba en `tests/test_acceptance.py`.

El workflow `.github/workflows/acceptance.yml` levanta PostgreSQL 16 en GitHub Actions, aplica las migraciones y ejecuta la suite completa en cada PR y push a `main`, archivando los reportes JUnit y HTML como artefactos descargables.
