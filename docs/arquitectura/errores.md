# Manejo de errores previsibles, validación y recuperación — Escenario 3 (COM-04)

Este documento especifica la estrategia de validación en los límites de la aplicación, el catálogo de errores semánticos seguros y la resiliencia ante fallos transitorios para el **Escenario 3**, en cumplimiento de **COM-04**.

---

## 1. Validación en el borde (HTTP Boundary Validation)

Todos los esquemas se procesan mediante **Pydantic v2** con rechazo estricto de campos adicionales no declarados (`extra="forbid"`):

- **`EventCreate`:**
  - `source` y `type`: Identificadores alfanuméricos ASCII `[A-Za-z0-9][A-Za-z0-9_-]*` (1 a 64 caracteres). Previene inyección SQL y traversal.
  - `external_id`: Cadena de 1 a 128 caracteres.
  - `occurred_at`: Fecha/hora RFC3339 obligatoriamente con huso horario (`timezone-aware`), normalizada de inmediato a UTC. Se rechazan enteros/timestamps Unix.
  - `payload`: Diccionario JSON objeto con serialización UTF-8 compacta que no debe sobrepasar **16 384 bytes (16 KiB)**.
- **`EventBatchCreate`:**
  - `events`: Colección de 1 a 100 objetos `EventCreate`. Se rechazan lotes vacíos o con más de 100 elementos. Validación de unicidad de claves naturales intra-lote antes de tocar el motor de base de datos.
- **`CountsQuery`:**
  - `from` y `to`: Fechas timezone-aware UTC que deben cumplir estrictamente `from < to`.
  - `group_by`: Restringido exclusivamente a los literales `"type"` o `"source"`.
- **`EventsListQuery`:**
  - Ventana temporal `from < to`.
  - Filtros opcionales `source` y `type` (1 a 64 caracteres alfanuméricos).
  - Paginación acotada: `limit` entre 1 y 100 (por defecto 50); `offset` entero mayor o igual a 0. Se rechazan explícitamente booleanos para evitar coerción implícita de Python.
- **`TimeseriesQuery`:**
  - `bucket`: Restringido a `"minute"`, `"hour"`, `"day"`.
  - Ventana acotada según bucket para prevenir agotamiento de memoria en base de datos: máximo 7 días para minuto, 90 días para hora, 366 días para día.
- **`SummaryQuery`:**
  - Ventana temporal `from < to` y filtros opcionales.
  - `numeric_field`: Identificador simple ASCII (1 a 64 caracteres, por defecto `"value"`). Previene ataques de inyección al extraer claves dinámicas del `JSONB`.

---

## 2. Catálogo uniforme de errores y seguridad de datos

Todo fallo devuelve un formato JSON estándar que no expone excepciones del sistema operativo, drivers de base de datos ni datos de la petición:

```json
{
  "error": {
    "code": "CODIGO_SEMANTICO",
    "message": "Mensaje seguro y comprensible.",
    "request_id": "c1f7a0b2-..."
  }
}
```

| Condición de falla | Código HTTP | `error.code` | Comportamiento y mitigación de seguridad |
|---|---:|---|---|
| Datos mal formados, tamaño excedido, ventana inválida | `422` | `INVALID_INPUT` | Mensaje fijo estándar. No se reflejan valores ni campos enviados en la respuesta ni en logs. |
| Clave de API ausente, vacía o no reconocida | `401` | `UNAUTHENTICATED` | Mensaje genérico sin confirmar existencia de claves. |
| Rol insuficiente o fuente no autorizada | `403` | `FORBIDDEN` | Acceso denegado; no revela detalles de permisos internos. |
| Identificador de evento inexistente (`UUID`) | `404` | `NOT_FOUND` | No copia el UUID en el cuerpo para evitar inyección en clientes. |
| Tupla `(source, external_id)` duplicada | `409` | `DUPLICATE` | Notifica conflicto sin sustituir ni alterar el registro existente. |
| Caída de PostgreSQL o timeout de conexión | `503` | `DEPENDENCY_UNAVAILABLE` | Error seguro sin exponer cadenas DSN, contraseñas ni hosts de la base. |
| Error inesperado del servidor | `500` | `INTERNAL_ERROR` | Oculta tracebacks, rutas locales y volcados de memoria. |

### Trazabilidad con `X-Request-ID`
- Toda respuesta HTTP incluye la cabecera `X-Request-ID`. Si el cliente envía un ID alfanumérico seguro (hasta 128 caracteres), se conserva; en caso contrario, se genera un UUID v4.
- Los logs estructurados del servidor registran únicamente: nivel de severidad, código de error, status HTTP, plantilla de la ruta (ej. `/v1/events/{event_id}`) y el `request_id`. Nunca se registran contraseñas, tokens, payloads ni cadenas de conexión.

---

## 3. Demostración real de caída y recuperación (200 → 503 → 200)

La evidencia de recuperación ante fallos de dependencias se demostró empíricamente sobre el proceso real de la API sin reiniciarlo:

1. Se inició la API con `uvicorn event_service.main:app` (PID conservado en memoria).
2. **Petición 1 (PostgreSQL operativo):** `GET /v1/analysis/counts` devolvió `status=200`, `code=OK`, `request_id=before-1`.
3. **Inducción de falla:** Se detuvo el contenedor PostgreSQL (`docker compose stop postgres`).
4. **Petición 2 (PostgreSQL caído):** `GET /v1/analysis/counts` devolvió de inmediato `status=503`, `code=DEPENDENCY_UNAVAILABLE`, `request_id=down-1`. La API se mantuvo viva (liveness OK).
5. **Restablecimiento:** Se reanudó el contenedor PostgreSQL (`docker compose start postgres`).
6. **Petición 3 (PostgreSQL recuperado):** La misma instancia de la API (mismo PID) atendió la consulta y devolvió `status=200`, `code=OK`, `request_id=after-1`.

```text
status= 200 request_id= before-1 code= OK
status= 503 request_id= down-1 code= DEPENDENCY_UNAVAILABLE
status= 200 request_id= after-1 code= OK
PASS: PostgreSQL recovered without restarting API
```
