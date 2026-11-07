# Validación, errores y recuperación · Rust / COM-04

`domain.rs` valida identificadores, UTC, tamaño UTF-8, lote y ventanas. Serde rechaza tipos/campos desconocidos. `service.rs` revalida y autoriza antes de cualquier repositorio. Los extractores Axum convierten JSON/query/path inválidos en errores uniformes; el cuerpo HTTP completo está acotado a 4 MiB. Contrato exacto en [API](../api.md).

`error.rs` concentra `InvalidInput`, `Unauthenticated`, `Forbidden`, `NotFound`, `Duplicate`, `Unavailable`, `Internal`. El middleware de `api.rs` agrega `X-Request-ID`, normaliza errores de routing/extracción y del catch-panic; nunca expone un mensaje del driver o del cliente.

```json
{"error":{"code":"DEPENDENCY_UNAVAILABLE","message":"Dependency unavailable.","request_id":"req-123"}}
```

Se acepta una única cabecera Request-ID segura de hasta 128 bytes. Si falta, es inválida o duplicada, se genera UUID v4. Logs JSON de `tracing`: ID, plantilla de ruta, código y estado; no URL real, query, body, cabeceras, stack ni DSN. No habilitar SQL logging con datos sensibles. Errores inesperados de tarea/proceso no equivalen a una garantía de disponibilidad; aborts de proceso y OOM requieren supervisión externa.

## Resiliencia

- `/health/live` no depende de DB; `/health/ready` hace `SELECT 1` por el pool compartido.
- Conexiones lazy: arrancar sin DB disponible no impide liveness.
- Pool verifica conexión antes de prestar; nuevas conexiones reciben UTC y timeout SQL.
- Reintentos limitados con backoff asíncrono solo en lecturas transitorias; nunca escrituras.
- El timeout por sentencia no es un deadline HTTP global: puede haber espera de pool y varios intentos. Configurar timeouts del proxy considerando esos límites.

## Experimento real `200 → 503 → 200`

Con configuración de prueba dedicada del README:

```bash
cargo build --locked --bin event-service
cargo run --locked --example verify_system
```

El programa crea PostgreSQL y API reales, conserva el PID API durante stop/start de DB, verifica 503 en analítica y 200 en liveness durante la caída, luego vuelve a 200 sin recrear API. También prueba reinicio completo para COM-03. Se detiene solo el contenedor creado por el experimento; el volumen permanece. Con Podman, `CONTAINER_ENGINE=podman`.

`tests/service.rs::dependency_recovers_without_rebuilding_app` usa un doble para test rápido; no debe presentarse como evidencia de reinicio real. La evidencia real la produce el experimento y el job CI `postgres`.
