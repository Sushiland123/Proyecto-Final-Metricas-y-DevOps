# Persistencia y durabilidad · Rust / SQLx / COM-03

PostgreSQL 16 es el custodio exclusivo de eventos. Esquema versionado en `migrations/`, embebido en el binario por `sqlx::migrate!`. Ver definición SQL real, sin una segunda copia divergente en documentación.

- UUID v4, `source VARCHAR(64)`, `external_id VARCHAR(128)`, `type VARCHAR(64)`.
- `occurred_at`, `ingested_at`: `TIMESTAMPTZ`, serializados en UTC.
- `payload JSONB`: objeto validado, máximo 16 KiB compacto UTF-8.
- Restricción `uq_events_source_external_id` e índices `(occurred_at,type)` y `(occurred_at,source)`.

## Transacciones y pool

`PostgresRepository` implementa `EventRepository`. Comparte un pool SQLx de máximo 10 conexiones con verificación antes de adquirir. Adquisición/conexión acotada por `DB_CONNECT_TIMEOUT_SECONDS`; `statement_timeout` aplicado a cada conexión. No se crean pools por petición.

Ingesta individual y lote usan INSERT parametrizado multi-fila y una transacción. Se devuelve 201 solo después de COMMIT. Error de inserción: rollback explícito; cancelación: rollback RAII. Únicamente `23505` sobre la restricción natural se traduce a DUPLICATE. No se reintentan escrituras: la pérdida de conexión durante COMMIT tiene resultado potencialmente ambiguo.

Lecturas transitorias reintentables (I/O/TLS, pool timeout, SQLSTATE 08*, reinicio DB, serialización/deadlock) aplican backoff exponencial con `tokio::time::sleep`; no bloquean hilos. Errores SQL no transitorios no se reintentan. Listado y resumen usan `REPEATABLE READ READ ONLY` para que sus consultas compartan snapshot.

## Migraciones y transición desde Python

```bash
cargo run --locked -- migrate
cargo run --locked -- migrate  # no-op si ya están aplicadas
```

SQLx registra versiones/checksums en `_sqlx_migrations` y usa locks de migración. No ejecutar migraciones en cada arranque/réplica; usar un Job controlado previo al despliegue. Cambios futuros requieren un archivo nuevo, nunca editar una migración ya aplicada.

Para una base del antiguo servicio:
1. Hacer backup y ensayar en una copia; detener escritores y herramientas antiguas durante la transición.
2. Cambiar DSN `postgresql+psycopg://` por `postgresql://`.
3. Ejecutar `event-service migrate`. La migración inicial usa `CREATE TABLE IF NOT EXISTS` y verifica columnas, tipos, nulabilidad, PK y restricción natural antes de adoptar una tabla existente. Si no coincide, aborta; no transforma ni elimina los datos.
4. Se conserva `alembic_version` como historial legado; no se usa como autoridad nueva ni se elimina. No ejecutar el gestor antiguo junto con SQLx.
5. Comparar datos y contratos HTTP, y conservar backup antes del corte definitivo.

Los índices se crean si faltan. Bases con índices homónimos pero definiciones alteradas necesitan revisión manual; `IF NOT EXISTS` no repara drift. Una migración fallida debe investigarse, no marcarse aplicada manualmente para saltar verificaciones.

## Verificación reproducible

- `cargo test --locked --test postgres -- --ignored`: concurrencia, rollback, lectura desde otro pool, migraciones repetidas y estructura/indexación.
- `cargo run --locked --example verify_system`: API real + PostgreSQL propio; escribe evento, comprueba outage/recovery en el mismo proceso, reinicia API y DB, migra dos veces y compara el documento completo.

Ver prerrequisitos y configuración aislada en el README. El experimento deja su contenedor detenido y volumen conservado para inspección; no hace `down -v`. La evidencia histórica de la implementación anterior permanece en Git (`6ef42ba`), no acredita la ejecución Rust. Los nuevos resultados se generan en CI y en `artifacts/` local.
