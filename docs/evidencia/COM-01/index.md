# COM-01 · Funciones y reglas · Rust

[Issue #1](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/1). Criterio: ingesta, exploración y análisis conforme al Escenario 3.

Reproducir con entorno PostgreSQL aislado del README:

```bash
cargo test --locked --all-targets
cargo test --locked --test postgres -- --ignored --nocapture
```

La [matriz E3-01…16](../../escenario.md) enlaza nombres de pruebas Rust. Integración verifica 201 y documento persistido, duplicados concurrentes, rollback de lote, filtros/paginación, ventanas UTC y agregados numéricos. También prueba adopción del esquema anterior sin reescribir datos y rechazo de esquemas incompatibles. Tras la revisión del [PR #39](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/pull/39), la ejecución local aprobó **29 pruebas: 10 PostgreSQL y 19 sin DB**, incluidas regresiones de `filters`, NUL y números extremos. Consultar los checks del commit vigente para el resultado CI actualizado.

Workflow `.github/workflows/acceptance.yml`, artefacto `rust-postgres-evidence`. La [primera ejecución CI del PR](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/actions/runs/42419564400) aprobó calidad, PostgreSQL/recuperación y contenedor sobre `8aaab61`. Los commits posteriores requieren sus propios checks. Evidencia de implementaciones anteriores archivada en Git, no presentada como ejecución Rust.
