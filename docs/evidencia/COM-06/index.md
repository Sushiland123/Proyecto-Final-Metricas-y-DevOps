# COM-06 — Evidencia de trade-offs y compromisos técnicos (Escenario 3)

**Issue:** [#6](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/6). Registro explícito de compromisos técnicos en consistencia, disponibilidad, costo, complejidad, control de acceso y analítica para el **Escenario 3**.

## 1. Documentación de compromisos

El análisis completo y las alternativas descartadas se encuentran en [docs/arquitectura/tradeoffs.md](../../arquitectura/tradeoffs.md).

Principales trade-offs registrados:
1. **Consistencia fuerte inmediata (ACID):** Confirmación tras `COMMIT` relacional (individual y batch con rollback atómico) vs cola asíncrona de consistencia eventual.
2. **Disponibilidad de nodo único:** Proceso web desacoplado y PostgreSQL con volumen montado en K3s vs clúster multizona HA.
3. **Costo controlado (< US$100):** Dimensionamiento eficiente en una VM Azure `Standard_B2s` con margen presupuestario vs múltiples servicios PaaS administrados.
4. **Complejidad contenida:** Monolito modular por capas cohesivas vs arquitectura distribuida de microservicios.
5. **Control de acceso enfocado:** RBAC y aislamiento lateral por origen mediante API Keys seguras vs Identity Provider OAuth2/OIDC externo.
6. **Analítica en tiempo real:** Agregaciones nativas con índices compuestos sobre tabla relacional vs base de datos columnar separada.

## 2. Coherencia con el código y despliegue

- Las decisiones están sustentadas en el código fuente (`src/event_service/`), las migraciones Alembic y la suite de pruebas.
- Se vinculan directamente con los registros [ADR-0001](../../adr/0001-descomposicion.md), [ADR-0002](../../adr/0002-almacenamiento.md) y [ADR-0003](../../adr/0003-comunicacion.md).
