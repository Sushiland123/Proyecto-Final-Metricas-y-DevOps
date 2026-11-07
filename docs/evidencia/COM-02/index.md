# COM-02 · Límites y arquitectura · Rust

[Issue #2](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/2). Criterio: responsabilidades y decisiones verificables en código.

- [Contexto](../../arquitectura/contexto.md), [módulos](../../arquitectura/contenedores.md), [despliegue](../../arquitectura/despliegue.md).
- ADRs [0001](../../adr/0001-descomposicion.md), [0002](../../adr/0002-almacenamiento.md), [0003](../../adr/0003-comunicacion.md), [0004 Rust](../../adr/0004-rust.md).
- `src/service.rs` depende del puerto `src/repository.rs`; `src/postgres.rs` implementa SQLx. `src/main.rs` construye e inyecta dependencias. `tests/service.rs` verifica autorización sin tocar el repositorio.

Reproducir: `cargo test --locked --test service`, `cargo doc --locked --no-deps`. Inspeccionar diagramas Mermaid contra módulos reales. La existencia del Dockerfile no acredita despliegue Azure/K3s ni revisión independiente; esos requisitos permanecen pendientes. Documentación exportada en artefacto CI `rust-api-and-code-docs`.

## Verificación local del contenedor · 2026-11-07

Se construyó el Dockerfile final con Podman: imagen `localhost/pfmdo-rust-review:latest`, ID `f329f5b235d988764cfaa8b8daff1706c205fa43390b7e24ef80688ad24c44ba`, tamaño local 93,962,548 bytes y usuario `10001:10001`. Se ejecutó el binario release del contenedor: exportó OpenAPI válido, atendió liveness con 200 y readiness con 503 sin DB, incluyendo error seguro y Request-ID. Es una prueba local, no publicación de imagen ni despliegue cloud.
