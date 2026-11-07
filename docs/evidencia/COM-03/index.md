# COM-03 · Persistencia y durabilidad · Rust

[Issue #3](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/3). Criterio: datos confirmados sobreviven a reinicios conservando volumen.

```bash
cargo test --locked --test postgres -- --ignored --nocapture
cargo build --locked --bin event-service
cargo run --locked --example verify_system
```

Configurar entorno dedicado según [README](../../../README.md). Integración `data_survives_new_pool_and_repeated_migrations` compara documento desde otro pool y comprueba versiones/índices: aprobada localmente en la migración Rust. El experimento de procesos crea contenedor propio, conserva volumen y compara evento completo tras reiniciar API y PostgreSQL; genera su evidencia al ejecutarse, sin reutilizar UUIDs/resultados de la versión anterior.

Esquema: `migrations/2026102101_events.sql`, `2026110202_source_time_index.sql`; control SQLx `_sqlx_migrations`. [Guía de adopción de esquema existente](../../arquitectura/persistencia.md). Artefactos CI: `rust-postgres-evidence` con logs de tests/experimento. Límite: no prueba pérdida de disco/VM, backup restore ni HA.

## Ejecución local Rust · 2026-11-07

Experimento ejecutado con PostgreSQL 16 y Podman:

```text
PASS: 200 -> 503 -> 200 without restarting API
PASS: event 0321b118-71c8-4a9b-8b09-44b728b30715 persisted after API + PostgreSQL restart
```

Se retuvo el volumen del experimento para inspección. Es una ejecución local, no un workflow GitHub ni un despliegue Azure.
