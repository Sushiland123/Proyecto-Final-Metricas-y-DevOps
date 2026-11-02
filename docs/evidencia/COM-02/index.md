# COM-02 — Evidencia de arquitectura y límites de componentes (Escenario 3)

**Requisito:** [Issue #2](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/2). Definición y justificación de límites, responsabilidades de componentes, decisiones técnicas (ADR) y diagramas de arquitectura reales.

## 1. Verificación del diseño arquitectónico

| Comprobación | Procedimiento de verificación | Resultado |
|---|---|---|
| **Arquitectura de componentes y módulos** | Inspección de `src/event_service/` (`api`, `schemas`, `auth`, `service`, `storage`, `contracts`, `errors`, `config`, `main`) y ejecución de pruebas unitarias y de integración. | Capas desacopladas; separación nítida entre HTTP, dominio y persistencia. Pruebas ejecutan con y sin base de datos real. |
| **Diagramas de arquitectura como código** | Diagramas Mermaid actualizados en [contexto](../../arquitectura/contexto.md), [contenedores](../../arquitectura/contenedores.md) y [despliegue](../../arquitectura/despliegue.md). | Diagramas validados conforme a la especificación del Escenario 3 (ingesta individual/batch, consultas y analítica). |
| **Registro de decisiones arquitectónicas (ADR)** | Consulta de [ADR-0001](../../adr/0001-descomposicion.md) (descomposición modular), [ADR-0002](../../adr/0002-almacenamiento.md) (PostgreSQL relacional) y [ADR-0003](../../adr/0003-comunicacion.md) (REST/SQL síncrono). | Cada decisión contiene contexto, justificación de elección, alternativas descartadas y consecuencias técnicas. |

## 2. Contraste de límites y contratos

- `event_service` es el único dueño lógico de las reglas del Escenario 3.
- `PostgreSQL 16` es el custodio físico de los datos con restricción única e índices compuestos.
- Los contratos y esquemas neutrales desacoplan los controladores HTTP del motor relacional.
