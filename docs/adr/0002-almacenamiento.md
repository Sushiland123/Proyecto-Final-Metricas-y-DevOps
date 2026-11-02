# ADR-0002 — PostgreSQL relacional para eventos y cálculo analítico

- **Fecha:** 2026-10-21 (Actualizado: 2026-11-02 para Escenario 3)
- **Estado:** Aceptada e implementada en el código base del Escenario 3.

## Contexto y decisión

El **Escenario 3** requiere garantizar unicidad de `(source, external_id)`, durabilidad ante reinicios con volumen conservado, consultas temporales en ventanas semiabiertas `[from, to)` y agregaciones estadísticas.

Elegimos **PostgreSQL 16** con **SQLAlchemy 2** y driver **psycopg 3**, gestionando el esquema exclusivamente con **Alembic**:
1. La tabla `events` almacena campos clave indexados y un campo `payload JSONB` de hasta 16 KiB.
2. La deduplicación se delega a la restricción `uq_events_source_external_id`.
3. Se implementan índices compuestos `ix_events_occurred_at_type` e `ix_events_occurred_at_source`.
4. Los cálculos de conteo, series de tiempo (`date_trunc`) y resúmenes estadísticos (`MIN`, `MAX`, `AVG`) se procesan directamente en la base de datos bajo demanda, sin persistir tablas derivadas redundantes.

## Alternativas y consecuencias

| Alternativa | Razón para no elegirla |
|---|---|
| Almacén en memoria o base de datos NoSQL sin transacciones | No ofrece garantías ACID suficientes para evitar duplicados concurrentes ni durabilidad estricta ante reinicios. |
| Base de datos columnar analítica separada (ClickHouse / Snowflake) | Aumenta la complejidad operativa, infraestructura y facturación en Azure sin ser necesaria para las consultas analíticas del escenario. |
| Vistas materializadas / ETL periódico | Introduce desfase temporal (eventual consistency) en los análisis e incrementa la complejidad de sincronización. |

**Ventajas:** Consistencia inmediata, garantías transaccionales ACID, soporte de JSON semiestructurado, durabilidad comprobada ante reinicios y bajo costo.
**Costos:** Dependencia operativa de un servicio relacional; mitigada con `pool_pre_ping`, timeouts configurados y reintentos automáticos con backoff en lecturas.
