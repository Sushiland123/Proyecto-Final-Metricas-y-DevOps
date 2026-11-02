# Escenario 3: Ingesta y Análisis de Eventos (COM-01)

Este documento especifica la implementación formal del **Escenario 3: Ingesta y Análisis de Eventos** en cumplimiento de los requisitos **COM-01 a COM-06** de la sección 3.1 (*Aplicación y arquitectura*).

La plataforma proporciona ingesta atómica e individual de telemetría y eventos, persistencia relacional durable con deduplicación por clave natural, segregación de acceso por rol y origen de emisión (aislamiento lateral), y capacidades analíticas sobre ventanas temporales (exploración paginada, agregación dimensional, series temporales por intervalos y resúmenes estadísticos).

---

## 1. Matriz de funciones y reglas del Escenario 3 (Regla → Prueba)

| ID Regla | Definición funcional y regla de negocio | Prueba de aceptación (`tests/test_acceptance.py`) | Pruebas complementarias |
|---|---|---|---|
| **E3-01** | **Ingesta puntual autenticada:** `POST /v1/events` valida origen, identificador externo, tipo, timestamp UTC y payload JSONB (máximo 16 KiB). Persiste en base relacional y devuelve `201 Created` con UUID del evento, datos confirmados y fecha UTC de ingestión (`ingested_at`). | `test_ingestion_returns_persisted_event` | `tests/test_errors.py::test_event_accepts_json_object_and_normalizes_utc` |
| **E3-02** | **Ingesta en lote atómica (Batch):** `POST /v1/events/batch` procesa listas de 1 a 100 eventos bajo una única transacción de base de datos. Si ocurre cualquier duplicado o error, la transacción se revierte íntegramente (rollback atómico) sin persistir eventos parciales. | `test_batch_ingestion_success_and_atomic_rollback_on_duplicate` | `tests/test_persistence.py::test_create_batch_atomicity_and_duplicate_handling`, `tests/test_errors.py::test_batch_schema_validation` |
| **E3-03** | **Deduplicación e idempotencia estricta:** La tupla `(source, external_id)` es única en el custodio de datos. Un reintento con la misma tupla devuelve `409 Conflict` (`DUPLICATE`) sin mutar el evento original ni duplicar conteos. Orígenes (`source`) distintos pueden reutilizar el mismo `external_id`. | `test_duplicate_does_not_modify_or_duplicate_first_event`, `test_external_id_can_repeat_in_another_source` | `tests/test_persistence.py::test_concurrent_duplicate_keeps_first_event` |
| **E3-04** | **Recuperación puntual por identificador:** `GET /v1/events/{id}` permite a un analista autenticado obtener el evento original por su UUID. Solicitudes con UUID inexistente devuelven `404 Not Found` público sin exponer internals. | `test_analyst_can_retrieve_event_and_missing_id_is_404` | `tests/test_errors.py::test_http_expected_and_unexpected_errors_are_uniform_and_safe` |
| **E3-05** | **Exploración temporal y paginación acotada:** `GET /v1/events` lista eventos dentro de una ventana semiabierta `[from, to)` con paginación (`limit` 1-100, `offset` >= 0) y filtros opcionales por `source` y `type`. Devuelve el total coincidente y la colección ordenada cronológicamente. | `test_events_listing_and_pagination` | `tests/test_persistence.py::test_list_events_pagination_and_filters`, `tests/test_errors.py::test_events_list_query_validation` |
| **E3-06** | **Agregación dimensional bajo demanda:** `GET /v1/analysis/counts` computa agrupaciones en tiempo real por `type` o `source` sobre una ventana `[from, to)`. Devuelve total global, buckets ordenados lexicográficamente y conteo exacto; ventana vacía devuelve total 0 y lista vacía. | `test_counts_by_type_and_source_are_sorted_and_empty_window_is_zero` | `tests/test_persistence.py::test_write_read_and_half_open_counts` |
| **E3-07** | **Análisis de series temporales (Bucketing):** `GET /v1/analysis/timeseries` agrupa temporalmente los eventos mediante funciones de truncado (`minute`, `hour`, `day`) con límites de ventana de seguridad (máximo 7 días para minuto, 90 días para hora, 366 días para día). | `test_timeseries_analytics` | `tests/test_persistence.py::test_timeseries_and_summary_analytics`, `tests/test_errors.py::test_timeseries_query_validation` |
| **E3-08** | **Resumen estadístico y análisis de métricas numéricas:** `GET /v1/analysis/summary` calcula total de eventos, conteo de fuentes distintas, tipos distintos, primer/último timestamp y agregaciones descriptivas (min, max, promedio) sobre campos numéricos del payload JSONB. | `test_summary_analytics` | `tests/test_persistence.py::test_timeseries_and_summary_analytics`, `tests/test_errors.py::test_summary_query_validation` |
| **E3-09** | **Ventana temporal semiabierta UTC:** Todas las fechas y consultas temporales son timezone-aware y se normalizan a UTC. Las ventanas `[from, to)` incluyen el extremo inicial y excluyen estrictamente el final (`occurred_at >= from AND occurred_at < to`), garantizando particionamiento adyacente sin solapes. | `test_counts_use_half_open_utc_window` | `tests/test_persistence.py::test_write_read_and_half_open_counts` |
| **E3-10** | **Control de acceso y segregación de privilegios:** Control de acceso basado en roles (RBAC): `producer` sólo puede ingestar (`events:create`), `analyst` sólo puede consultar y analizar (`events:read`, `analysis:read`). Las credenciales se envían en `X-API-Key` y se validan tanto en borde HTTP como en la capa de servicio de dominio. Claves ausentes o erróneas devuelven `401 Unauthorized`; rol incorrecto devuelve `403 Forbidden`. | `test_protected_routes_require_correct_role`, `test_service_rechecks_permission_before_repository_access` | `tests/test_auth.py::test_authorized_route`, `test_valid_key_cannot_escalate_to_other_role` |
| **E3-11** | **Aislamiento lateral por fuente (Source Scoping):** Soporte de claves de productor restringidas a una o más fuentes específicas (`EVENT_PRODUCER_SOURCE_KEYS`). Un productor asignado a `sensor-a` no puede emitir eventos en nombre de `sensor-b` (`403 Forbidden`). | `tests/test_auth.py::test_source_scoped_producer_authorization` | `tests/test_auth.py::test_malformed_or_colliding_source_keys_fail_closed` |
| **E3-12** | **Validación estricta de entradas y rechazo de sobrecargas:** Rechazo de identificadores con caracteres especiales (inyección/path traversal), rechazo de campos adicionales (`extra="forbid"`), validación de rangos numéricos y ventanas invertidas `from >= to` (`422 Unprocessable Entity`). Cargas útiles limitadas estrictamente a 16 KiB. | `test_invalid_count_window_is_rejected`, `test_ingestion_rejects_invalid_fields`, `test_counts_reject_invalid_query` | `tests/test_errors.py::test_payload_accepts_16_kib_and_rejects_one_byte_more` |
| **E3-13** | **Respuestas públicas uniformes y sanitización:** Todo fallo devuelve JSON estructurado `{"error":{"code":"...","message":"...","request_id":"..."}}` con código semántico. Los errores nunca exponen secretos, variables de entorno, DSNs de base de datos ni volcados de stack (traceback). `X-Request-ID` correlaciona respuesta con logs estructurados sin filtrar información sensible. | `tests/test_errors.py::test_http_expected_and_unexpected_errors_are_uniform_and_safe`, `test_logs_route_template_not_client_supplied_path_value` | `tests/test_auth.py::test_credentials_never_appear_in_responses_or_logs` |
| **E3-14** | **Resiliencia ante caídas y recuperación sin reinicio:** La API maneja caídas de PostgreSQL devolviendo `503 Service Unavailable` sin crashear. Al recuperarse la conectividad, las consultas vuelven a responder `200 OK` en el mismo proceso de aplicación mediante `pool_pre_ping`, timeouts acotados y reintentos automáticos con backoff en lecturas. | `tests/test_errors.py::test_dependency_recovery_without_recreating_app_or_client` | Procedimiento real en `docs/arquitectura/errores.md` |
| **E3-15** | **Supervivencia a reinicios y durabilidad de datos:** La base conserva los datos y el esquema ante reinicio de contenedores o pods mediante volumen persistente y migraciones versionadas idempotentes con Alembic. | `tests/test_persistence.py::test_data_survives_new_engine_and_repeated_upgrade` | Script reproducible `scripts/verify_durability.py` |
| **E3-16** | **Sondas de ciclo de vida:** `/health/live` indica que el proceso está en ejecución sin depender de la base; `/health/ready` realiza una prueba activa `SELECT 1` con timeout de 3 segundos para confirmar que el componente está listo para recibir tráfico. | `test_health_endpoints_are_public` | `tests/test_skeleton.py::test_ready_checks_select_one_when_database_is_configured` |

---

## 2. Reproducción de la suite de pruebas

Para reproducir localmente contra una instancia PostgreSQL 16 aislada de pruebas:

```bash
uv sync --locked
cat > .env <<EOF
POSTGRES_USER=events
POSTGRES_DB=events_test
POSTGRES_PORT=15433
POSTGRES_PASSWORD=$(openssl rand -hex 16)
EVENT_PRODUCER_KEY=$(openssl rand -hex 16)
EVENT_ANALYST_KEY=$(openssl rand -hex 16)
EVENT_PRODUCER_SOURCE_KEYS=sensor-a:$(openssl rand -hex 16),sensor-b:$(openssl rand -hex 16)
EOF
set -a; . ./.env; set +a
cat >> .env <<EOF
DATABASE_URL=postgresql+psycopg://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}
TEST_DATABASE_URL=postgresql+psycopg://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}
EOF
set -a; . ./.env; set +a
chmod 600 .env

# Levantar contenedor dedicado de pruebas
docker compose up -d postgres

# Aplicar migraciones versionadas (incluye tabla e índices optimizados)
uv run --locked alembic upgrade head

# Ejecutar suite completa con reporte JUnit y HTML
mkdir -p artifacts
uv run --locked pytest --junitxml=artifacts/junit.xml --html=artifacts/acceptance.html --self-contained-html
```

---

## 3. Cobertura de Requisitos Técnicos Comunes (COM-01 a COM-06)

- **COM-01 (Funciones y reglas):** Todas las reglas E3-01 a E3-16 cuentan con pruebas de aceptación automatizadas en `tests/test_acceptance.py`.
- **COM-02 (Límites y responsabilidades):** Diagramas C4 en `docs/arquitectura/` y ADRs 0001, 0002 y 0003 documentando límites entre HTTP, Dominio, Repositorio y PostgreSQL.
- **COM-03 (Persistencia):** Modelo relacional en PostgreSQL 16 con Alembic, unicidad `(source, external_id)`, volumen nombrado y validación de supervivencia ante reinicio con `scripts/verify_durability.py`.
- **COM-04 (Manejo de errores y recuperación):** Matriz completa de validación, error codes uniformes, `request_id` y resiliencia `200 -> 503 -> 200` demostrada sin reiniciar el proceso.
- **COM-05 (Control de acceso):** Segregación RBAC (`producer`/`analyst`) y aislamiento lateral por origen (`EVENT_PRODUCER_SOURCE_KEYS`) con validación en ruta y en servicio.
- **COM-06 (Compromisos y trade-offs):** Justificación de consistencia transaccional inmediata vs eventual, disponibilidad con nodo único K3s, control presupuestario bajo US$100 y complejidad contenida.
