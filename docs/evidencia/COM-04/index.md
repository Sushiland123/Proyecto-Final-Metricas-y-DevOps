# COM-04 · Errores, validación y recuperación · Rust

[Issue #4](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/4). Criterio: 422/409/404/503 seguros, Request-ID y recuperación sin recrear API.

```bash
cargo test --locked --test domain --test http --test service
# Contra contenedor dedicado y configuración del README:
cargo build --locked --bin event-service
cargo run --locked --example verify_system
```

Pruebas verifican identificadores, fechas con zona, tamaños UTF-8 exactos, lotes y ventanas; HTTP comprueba mensajes fijos, ausencia del valor recibido, IDs inválidos/duplicados, JSON roto y routing. `service` comprueba recuperación con doble de repositorio; no sustituye el experimento real de DB.

El experimento real detiene su DB, verifica 503 en datos y 200 en liveness, reinicia DB y obtiene 200 conservando PID API. Resultado y logs se producen en `rust-postgres-evidence` al ejecutarlo; no se reutilizan demostraciones históricas. [Diseño y límites](../../arquitectura/errores.md): timeouts por operación, no deadline HTTP global.

Ejecución local Rust del 2026-11-07 con Podman/PostgreSQL 16: `PASS: 200 -> 503 -> 200 without restarting API`. El mismo experimento verificó supervivencia al reinicio completo; [resultado COM-03](../COM-03/index.md). CI debe repetirlo sobre el commit del PR.

Se ejecutó además la variante del experimento que inyecta marcadores sensibles en payload/URL y compara logs reales: `PASS: failure responses and API logs do not disclose submitted secrets or URL values`.
