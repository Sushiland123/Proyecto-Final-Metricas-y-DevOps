# Persistencia y durabilidad de datos — Escenario 3 (COM-03)

Este documento describe la arquitectura de persistencia, modelado de datos relacional y garantías de durabilidad implementadas para el **Escenario 3: Ingesta y Análisis de Eventos**, en cumplimiento de **COM-03**.

---

## 1. Modelo de Datos y Esquema Relacional

La persistencia se implementa sobre **PostgreSQL 16** gestionada mediante **SQLAlchemy 2** y el driver nativo **psycopg 3**. La tabla principal `events` está versionada exclusivamente mediante migraciones controladas de **Alembic**:

```sql
CREATE TABLE events (
    id UUID PRIMARY KEY,
    source VARCHAR(64) NOT NULL,
    external_id VARCHAR(128) NOT NULL,
    type VARCHAR(64) NOT NULL,
    occurred_at TIMESTAMPTZ NOT NULL,
    ingested_at TIMESTAMPTZ NOT NULL,
    payload JSONB NOT NULL,
    CONSTRAINT uq_events_source_external_id UNIQUE (source, external_id)
);

CREATE INDEX ix_events_occurred_at_type ON events (occurred_at, type);
CREATE INDEX ix_events_occurred_at_source ON events (occurred_at, source);
```

### Características del diseño
1. **Identidad y deduplicación estricta:** `id` es un UUID v4 generado por el servicio. La restricción única `uq_events_source_external_id` garantiza a nivel de motor relacional que un mismo par `(source, external_id)` no pueda duplicarse, incluso ante escrituras altamente concurrentes.
2. **Normalización temporal a UTC:** Tanto `occurred_at` (evento de negocio) como `ingested_at` (marca de tiempo del sistema) se persisten como `TIMESTAMPTZ` y se normalizan a UTC.
3. **Carga útil semiestructurada (`JSONB`):** Permite almacenar cargas arbitrarias de hasta 16 KiB validadas en el borde, indexables y consultables directamente mediante operadores JSON de PostgreSQL (como `jsonb_typeof`, extracción de campos y cálculo de promedios/máximos/mínimos sin esquemas rígidos adicionales).
4. **Índices compuestos optimizados:**
   - `ix_events_occurred_at_type`: Acelera filtros y agrupaciones temporales por tipo de evento (`GET /v1/analysis/counts?group_by=type` y `GET /v1/events?type=...`).
   - `ix_events_occurred_at_source`: Acelera consultas por ventana temporal restringidas a fuentes específicas (`GET /v1/analysis/counts?group_by=source` y `GET /v1/events?source=...`).

---

## 2. Transaccionalidad y Ciclo de Vida de Conexiones

- **Manejo de sesiones por solicitud:** La dependencia FastAPI `get_repository` genera una sesión SQLAlchemy dedicada por petición HTTP y asegura su cierre al terminar el contexto.
- **Rollback atómico:** Si ocurre cualquier excepción durante una escritura puntual o en lote (`POST /v1/events/batch`), el repositorio ejecuta `session.rollback()`. En caso de duplicados, captura la excepción de integridad `23505` y la traduce limpiamente a `DuplicateEvent`.
- **Pre-ping de conexiones:** El pool de conexiones se crea con `pool_pre_ping=True`. Si el contenedor o pod de PostgreSQL se reinicia o se pierde la conexión TCP, SQLAlchemy descarta conexiones obsoletas antes de utilizarlas, eliminando la necesidad de reiniciar el proceso de la aplicación.
- **Timeouts acotados:** Las conexiones se configuran con `connect_timeout=3` segundos y `statement_timeout=5000` ms para evitar que consultas lentas o bloqueadas degraden la disponibilidad general.
- **Reintentos idempotentes con backoff en lecturas:** Consultas de lectura (`get`, `list_events`, `counts`, `timeseries`, `summary`) aplican un reintento automático con backoff exponencial ante fallos transitorios de red (`OperationalError`).

---

## 3. Demostración de Persistencia ante Reinicio de Componentes

La evidencia mínima exigida por **COM-03** consiste en demostrar que los datos sobreviven al reinicio de los componentes:

### Procedimiento automatizado (`scripts/verify_durability.py`):
1. Levanta un contenedor dedicado de PostgreSQL con volumen nombrado (`postgres_data`).
2. Aplica las migraciones de Alembic (`alembic upgrade head`).
3. Inicia la API y escribe un evento con un `external_id` generado.
4. **Detiene completamente el proceso de la API.**
5. **Detiene completamente el contenedor de PostgreSQL (`docker compose stop postgres`), conservando el volumen.**
6. Vuelve a iniciar el contenedor de PostgreSQL.
7. Aplica nuevamente `alembic upgrade head` dos veces sucesivas (demostrando idempotencia de migraciones sin pérdida de datos).
8. Inicia un nuevo proceso de la API en un puerto dedicado.
9. Recupera el evento persistido mediante `GET /v1/events/{id}` y ejecuta una consulta agregada `GET /v1/analysis/summary`.
10. Comprueba byte a byte la integridad de todos los campos persistidos.

**Resultado obtenido en ejecución verificable:**
```text
PASS: event a8e98163-4488-4e3f-afc1-a6a181b81163 persisted after API + PostgreSQL restart
```

---

## 4. Límites y Supuestos

- Se demuestra la supervivencia ante reinicio de procesos de aplicación y reinicio de contenedores/pods con volumen montado. No se simula la pérdida total del almacenamiento físico de la VM o zona de disponibilidad de Azure.
- El esquema no requiere ni implementa almacenamiento efímero en memoria para datos de negocio: todo evento aceptado queda confirmado en PostgreSQL antes de devolver el código HTTP `201`.
