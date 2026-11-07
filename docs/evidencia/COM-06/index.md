# COM-06 · Trade-offs y decisiones · Rust

[Issue #6](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/6). Criterio: decisiones explícitas sobre consistencia, disponibilidad, coste y complejidad.

Ver [registro de trade-offs](../../arquitectura/tradeoffs.md) y [ADR de plataforma Rust](../../adr/0004-rust.md).

1. ACID/201 tras COMMIT frente a cola con aceptación diferida.
2. Monolito modular con puerto de repositorio frente a microservicios/framework DI.
3. SQLx async y pool acotado, sin cambiar la autoridad de PostgreSQL.
4. API keys por rol/fuente frente a un IdP no requerido.
5. Plataforma íntegramente Rust, con compilación, pruebas y herramientas de verificación bajo Cargo; mejoras cuantitativas sujetas a benchmark.
6. Objetivo Azure < US$100 sujeto a validación de infraestructura, cargos y políticas de uso.

Contrastar con `src/`, `migrations/`, pruebas Rust y CI. Los resultados locales no acreditan despliegue Azure, cumplimiento de SLA ni capacidad sostenida de producción. El historial de decisiones se conserva en Git.
