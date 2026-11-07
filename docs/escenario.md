# Escenario 3 · Ingesta y análisis de eventos · Rust

Especificación COM-01…06 de la implementación íntegramente Rust. Contrato detallado y ejemplos en [API](api.md). La matriz identifica pruebas ejecutables y distingue verificación de contratos, integración y experimentos operativos.

## Matriz regla → prueba

| Regla | Contrato | Prueba principal |
|---|---|---|
| E3-01 | Ingesta individual validada, UUID, UTC y 201 tras COMMIT | `tests/postgres.rs::ingestion_get_duplicate_and_pagination` |
| E3-02 | Lote 1–100, todo o nada; verifica alcance completo antes de escribir | `tests/postgres.rs::batch_is_atomic_and_source_scope_is_enforced`, `tests/domain.rs::batch_bounds_and_unknown_fields` |
| E3-03 | Unicidad `(source,external_id)`, 409 sin mutar original; fuentes distintas pueden reutilizar ID | `tests/postgres.rs::concurrent_duplicate_has_exactly_one_winner`, `external_id_can_repeat_in_different_source`, `ingestion_get_duplicate_and_pagination` |
| E3-04 | GET por UUID, 404 si falta; UUID inválido 422 | `tests/postgres.rs::ingestion_get_duplicate_and_pagination`, `tests/http.rs::health_and_uniform_safe_errors` |
| E3-05 | Listado cronológico con filtros, limit 1–100, offset 0–100000 | `tests/postgres.rs::ingestion_get_duplicate_and_pagination`, `tests/domain.rs::validates_queries_and_window_limits` |
| E3-06 | Conteos por tipo/fuente, claves ordenadas, vacío = 0 | `tests/postgres.rs::counts_use_half_open_utc_windows_and_sorted_buckets` |
| E3-07 | Buckets UTC minute/hour/day; máximo 7/90/366 días | `tests/postgres.rs::timeseries_summary_and_empty_results`, `tests/domain.rs::validates_queries_and_window_limits` |
| E3-08 | Resumen numérico ignora strings, booleanos, null y magnitudes fuera de float64; sin muestras estadísticas null | `tests/postgres.rs::timeseries_summary_and_empty_results`, `summary_handles_extreme_numeric_payloads_without_losing_events` |
| E3-09 | RFC3339 con zona normalizado UTC; ventana semiabierta `[from,to)` | `tests/postgres.rs::counts_use_half_open_utc_windows_and_sorted_buckets`, `tests/domain.rs::event_normalizes_utc_and_preserves_json` |
| E3-10 | RBAC producer/analyst; autorización en casos de uso antes de persistencia | `tests/service.rs::service_rechecks_permissions_before_repository_access`, `tests/auth.rs::roles_and_source_scope_cannot_escalate` |
| E3-11 | Claves restringidas por fuente y fallo cerrado por configuración ambigua | `tests/auth.rs::malformed_or_colliding_settings_fail_closed`, `one_scoped_key_may_authorize_multiple_sources` |
| E3-12 | Campos/tipos estrictos, identificadores, fechas, payload UTF-8 16 KiB | `tests/domain.rs::rejects_invalid_identifiers_dates_payloads_and_extra_fields`, `payload_limit_counts_utf8_bytes` |
| E3-13 | Error JSON seguro, sin datos recibidos; Request-ID validado en cada respuesta | `tests/http.rs::health_and_uniform_safe_errors`, `request_id_is_preserved_or_replaced_and_duplicate_keys_rejected`, `rejects_malformed_json_unknown_query_and_wrong_methods` |
| E3-14 | DB caída: 503, liveness 200; recuperación sin reinicio API | `examples/verify_system.rs`; doble rápido en `tests/service.rs::dependency_recovers_without_rebuilding_app` |
| E3-15 | Durabilidad con volumen, migraciones repetidas y nuevo proceso/pool | `tests/postgres.rs::data_survives_new_pool_and_repeated_migrations`, `examples/verify_system.rs` |
| E3-16 | Sondas públicas live y ready con SELECT 1 | `tests/http.rs::health_and_uniform_safe_errors`, `tests/postgres.rs::ingestion_get_duplicate_and_pagination` |

## Reproducción y límites

Seguir entorno aislado del [README](../README.md): `cargo test --locked --all-targets`, luego `cargo test --locked --test postgres -- --ignored`. El job `postgres` de CI ejecuta integración obligatoriamente. Faltar `TEST_DATABASE_URL` al pedirla hace fallar, no saltar silenciosamente. No se truncan bases; datos por fuente UUID.

Experimento de procesos: `cargo build --locked --bin event-service` y `cargo run --locked --example verify_system` con otro puerto y configuración dedicada. No confundir prueba con doble de repositorio con evidencia real de DB. El volumen prueba reinicios, no pérdida del disco/VM; no existe HA ni backup automatizado.

COM-02: [módulos](arquitectura/contenedores.md) y [ADRs](adr/0001-descomposicion.md). COM-03: [persistencia](arquitectura/persistencia.md). COM-04: [errores](arquitectura/errores.md). COM-05: [acceso](arquitectura/acceso.md). COM-06: [trade-offs](arquitectura/tradeoffs.md). Infraestructura cloud y presupuesto siguen sujetos al backlog DEV.
