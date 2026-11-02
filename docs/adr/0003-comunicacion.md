# ADR-0003 — Protocolo HTTP REST y consultas SQL sincrónicas

- **Fecha:** 2026-10-21 (Actualizado: 2026-11-02 para Escenario 3)
- **Estado:** Aceptada e implementada en el código base del Escenario 3.

## Contexto y decisión

Los clientes y sensores del **Escenario 3** requieren confirmación inmediata sobre si un evento (o lote de eventos) fue aceptado, duplicado o rechazado por formato, y los consumidores analíticos necesitan consultar datos actualizados en tiempo real.

Elegimos **HTTP/REST sincrónico con formato JSON** para la interfaz cliente-servicio y **SQL sincrónico sobre pool de conexiones** para la interacción servicio-PostgreSQL:
- `POST /v1/events` y `POST /v1/events/batch` confirman la transacción antes de responder `201 Created`.
- Consultas `GET /v1/events`, `GET /v1/analysis/*` leen directamente de la base de datos y devuelven el estado más reciente.
- No se incorpora un broker de mensajes intermedio por defecto.

## Alternativas y consecuencias

| Alternativa | Razón para no elegirla |
|---|---|
| Cola de mensajes asíncrona (RabbitMQ, Kafka, Azure Service Bus) | Complica la confirmación atómica al cliente (devolvería 202 Accepted en lugar de confirmar persistencia real) y añade infraestructura costosa. |
| gRPC / Protocol Buffers | Mayor complejidad de inspección y herramientas cliente; HTTP/JSON cumple holgadamente los requisitos de latencia y facilidad de prueba con curl/pytest. |

**Ventajas:** Simplicidad conceptual, trazabilidad directa con `X-Request-ID`, respuestas inmediatas de duplicados (409) o errores de entrada (422) y facilidad de prueba.
**Costos:** La latencia de ingesta depende directamente del tiempo de confirmación en base de datos; mitigado mediante soporte de inserción atómica en lote (`POST /v1/events/batch`).
