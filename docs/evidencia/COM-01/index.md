# COM-01 · Funciones y reglas · Rust

[Issue #1](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/1). Criterio: ingesta, exploración y análisis conforme al Escenario 3.

Reproducir con entorno PostgreSQL aislado del README:

```bash
cargo test --locked --all-targets
cargo test --locked --test postgres -- --ignored --nocapture
```

La [matriz E3-01…16](../../escenario.md) enlaza nombres de pruebas Rust. Integración verifica 201 y documento persistido, duplicados concurrentes, rollback de lote, filtros/paginación, ventanas UTC y agregados numéricos. También prueba adopción del esquema anterior sin reescribir datos y rechazo de esquemas incompatibles. Resultado local de esta migración: **9 pruebas PostgreSQL aprobadas**, sin ignoradas al solicitar integración, y **19 pruebas sin DB** aprobadas. CI debe repetirlas sobre el commit integrado.

Workflow `.github/workflows/acceptance.yml`, artefacto `rust-postgres-evidence`. No se enlaza una ejecución GitHub aún inexistente. Evidencia anterior archivada en Git `6ef42ba`, no presentada como ejecución Rust.
