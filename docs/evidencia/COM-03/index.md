# COM-03 — Evidencia de persistencia y durabilidad (Escenario 3)

**Criterio:** La información persistida por el Escenario 3 sobrevive al reinicio de los procesos de la aplicación y del contenedor/pod de base de datos. Las migraciones están versionadas y son idempotentes. Véase [issue #3](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/3).

## 1. Procedimiento de prueba y verificación

Con PostgreSQL 16 y volumen nombrado persistente:

```bash
uv sync --locked
uv run --locked alembic upgrade head
uv run --locked pytest tests/test_persistence.py
```

## 2. Experimento real de reinicio de componentes

Ejecutando el script de durabilidad `scripts/verify_durability.py`:
1. Crea un evento por API contra PostgreSQL.
2. Detiene el proceso de API.
3. Detiene el contenedor PostgreSQL (`docker compose stop postgres`), reteniendo el volumen nombrado.
4. Reinicia PostgreSQL y reaplica las migraciones de Alembic dos veces consecutivas.
5. Inicia un proceso nuevo de API.
6. Lee el evento por UUID y ejecuta una consulta agregada `GET /v1/analysis/summary`.

**Resultado obtenido:**
```text
PASS: event a8e98163-4488-4e3f-afc1-a6a181b81163 persisted after API + PostgreSQL restart
```

## 3. Versionamiento de esquema

- Migración `20261021_01_create_events.py`: Creación de la tabla `events`, clave primaria UUID, restricción única `uq_events_source_external_id` e índice `ix_events_occurred_at_type`.
- Migración `20261102_02_add_source_time_index.py`: Adición del índice compuesto `ix_events_occurred_at_source` para optimizar consultas del Escenario 3.
- Idempotencia verificada en `test_data_survives_new_engine_and_repeated_upgrade`.
