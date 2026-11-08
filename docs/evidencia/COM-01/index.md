# COM-01 · Funciones y reglas · Rust

[Issue #1](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/1). Criterio: ingesta, exploración y análisis conforme al Escenario 3.

Reproducir con entorno PostgreSQL aislado del README:

```bash
cargo test --locked --all-targets
cargo test --locked --test postgres -- --ignored --nocapture
```

La [matriz ESC3-01…10](../../escenario.md#matriz-de-cobertura) distingue el escenario
oficial de la API genérica anterior. `tests/scenario3/mod.rs` se ejecuta dentro del
target PostgreSQL existente: 1,000 válidos, prueba adicional con 50 duplicados y 50
tardíos, rechazos, concurrencia, agregados, límites, alertas y muerte/recuperación
de un procesador real con backlog. `examples/verify_system.rs` verifica reinicio
real de API y PostgreSQL con datos ESC3, no solo eventos genéricos.

Para la carga de 2,400 únicos a 20/s durante 120 s usar `scenario_load sustained`;
los perfiles burst/extra/custom, comandos y formato de evidencia están en la guía.
No sustituir esa ejecución por una prueba unitaria ni afirmar que se ejecutó en Azure
si el reporte corresponde al entorno local.

Resultados funcionales/carga y límites de la ejecución local del 2026-11-08:
[evidencia ESC3](../ESC3/index.md).

Workflow `.github/workflows/acceptance.yml`, artefacto `rust-postgres-evidence`. La [primera ejecución CI del PR](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/actions/runs/42419564400) aprobó calidad, PostgreSQL/recuperación y contenedor sobre `8aaab61`. Los commits posteriores requieren sus propios checks. Evidencia de implementaciones anteriores archivada en Git, no presentada como ejecución Rust.
