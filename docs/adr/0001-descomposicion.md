# ADR-0001 — Aplicación modular para el Escenario 3

- **Fecha:** 2026-10-21 (Actualizado: 2026-11-02 para Escenario 3)
- **Estado:** Aceptada e implementada en el código base del Escenario 3.

## Contexto y decisión

El **Escenario 3: Ingesta y Análisis de Eventos** exige ingesta puntual y en lote, persistencia relacional con deduplicación por clave natural, consultas por identificador, exploración paginada y múltiples capacidades analíticas (conteos por dimensiones, series temporales y resúmenes estadísticos).

Elegimos una **aplicación FastAPI modular** estructurada en capas como unidad de despliegue principal (`api`, `schemas`, `auth`, `service`, `storage`, `contracts`, `errors`, `config`):
- La capa de servicio (`service.py`) es la autoridad lógica sobre las reglas del Escenario 3 y la revalidación de permisos.
- La capa de almacenamiento (`storage.py`) es el adaptador exclusivo hacia PostgreSQL 16.
- El controlador (`api.py`) y los esquemas (`schemas.py`) aíslan el transporte HTTP de la lógica de dominio.

## Alternativas y consecuencias

| Alternativa | Razón para no elegirla |
|---|---|
| Servicios independientes de ingesta y analítica | Introduciría latencia de red, contratos de sincronización entre servicios y costos adicionales de despliegue sin necesidad operativa justificada para la escala requerida. |
| Monolito sin capas internas (código plano) | Dificultaría las pruebas unitarias independientes, acoplaría el transporte HTTP a detalles de la base de datos y aumentaría el riesgo de fuga de excepciones internas. |

**Ventajas:** Cohesión alta, bajo acoplamiento, simplicidad de pruebas automatizadas, menor consumo de recursos en Azure (< US$100) y cero latencia de red interna.
**Costos:** Escalado horizontal conjunto de ingesta y consultas; mitigado mediante diseño sin estado del servicio web y conexiones pooled.
