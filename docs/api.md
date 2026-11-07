# API HTTP · Event Service en Rust

Base local: `http://127.0.0.1:8000`. JSON UTF-8. Contrato estructurado en `/openapi.json`, generado desde `src/api.rs` y `src/domain.rs`. Visor en `/docs`.

## Autenticación

Enviar `X-API-Key`. `producer` permite ingesta, `analyst` permite lecturas y análisis. Las claves por fuente solo conceden ingesta para las fuentes asociadas. Sin clave válida: 401; rol/fuente insuficiente: 403. Cabeceras API-key duplicadas se rechazan. No enviar secretos en URLs.

## Evento

```json
{
  "source": "sensor-a",
  "external_id": "e-001",
  "type": "temperature",
  "occurred_at": "2026-10-21T12:00:00Z",
  "payload": {"value": 23.5}
}
```

- `source`, `type`: `[A-Za-z0-9][A-Za-z0-9_-]*`, 1–64 caracteres ASCII.
- `external_id`: cadena no vacía, máximo 128 caracteres Unicode.
- `occurred_at`: RFC3339 con zona, nunca timestamp numérico ni fecha sin offset. Respuestas normalizadas a UTC.
- `payload`: objeto JSON, máximo 16384 bytes al serializar compacto UTF-8. Se aceptan objetos/arreglos anidados, booleanos y null; no NaN ni Infinity.
- Los números JSON se conservan sin conversión silenciosa a float64 durante ingesta/lectura (`serde_json/arbitrary_precision`); aplican los límites de representación JSONB de PostgreSQL. Las estadísticas sí se devuelven como float64.
- Campos adicionales rechazados en eventos, envoltura de lote y query params.
- La respuesta añade `id` UUID v4 e `ingested_at` UTC. `(source, external_id)` es único; un conflicto no reemplaza el evento existente.

## Rutas

| Método y ruta | Rol | Respuesta exitosa |
|---|---|---|
| `POST /v1/events` | producer | 201, evento completo confirmado |
| `POST /v1/events/batch` | producer | 201, `{"inserted":2,"events":[...]}` |
| `GET /v1/events/{id}` | analyst | 200, evento; UUID inexistente: 404 |
| `GET /v1/events` | analyst | 200, `total`, `limit`, `offset`, `window`, `items` |
| `GET /v1/analysis/counts` | analyst | 200, `total`, `group_by`, `window`, `buckets:[{key,count}]` |
| `GET /v1/analysis/timeseries` | analyst | 200, `total`, `bucket`, `window`, `filters`, `points:[{bucket_start,count}]` |
| `GET /v1/analysis/summary` | analyst | 200, resumen descrito abajo |
| `GET /health/live` | público | 200, `{"status":"alive"}` independiente de DB |
| `GET /health/ready` | público | 200, `{"status":"ready"}` tras `SELECT 1`; DB caída: 503 |
| `GET /openapi.json`, `GET /docs` | público | Contrato generado y visor |

### Lotes

```bash
curl -H "X-API-Key: $EVENT_PRODUCER_KEY" -H 'Content-Type: application/json' \
  -d '{"events":[{"source":"sensor-a","external_id":"e-002","type":"temperature","occurred_at":"2026-10-21T12:05:00Z","payload":{"value":24}}]}' \
  http://127.0.0.1:8000/v1/events/batch
```

`events` contiene 1–100 elementos. Todos confirman juntos; duplicados internos o contra DB provocan 409 y rollback completo. Se conserva el orden de entrada en la respuesta. Se verifican todas las fuentes antes de escribir. Nunca se reintenta automáticamente una escritura: si se pierde la conexión durante COMMIT, verificar el resultado mediante consulta; reenviar puede devolver 409.

### Consultas

Todas las consultas de listado/análisis requieren `from` y `to`, RFC3339 con zona y `from < to`. Ventana semiabierta UTC `[from,to)`. Para offsets `+02:00`, usar `--data-urlencode` (no un `+` literal en URL).

| Ruta | Parámetros adicionales |
|---|---|
| `/v1/events` | `source`, `type` opcionales; `limit` 1–100 (50), `offset` 0–100000 (0) |
| `/v1/analysis/counts` | `group_by=type|source` obligatorio; no admite filtros adicionales |
| `/v1/analysis/timeseries` | `source`, `type` opcionales; `bucket=minute|hour|day` (hour) |
| `/v1/analysis/summary` | `source`, `type` opcionales; `numeric_field` identificador simple (value) |

Series: máximo 7 días con `minute`, 90 con `hour`, 366 con `day`. Solo devuelve buckets con datos, truncados en UTC; no rellena ceros. Conteos y resúmenes no limitan el rango temporal: el timeout SQL limita cada sentencia, no el coste total de exploración. Listados se ordenan por `occurred_at ASC, id ASC`; conteos por clave ascendente.

```bash
curl -G -H "X-API-Key: $EVENT_ANALYST_KEY" \
  --data-urlencode 'from=2026-10-21T00:00:00Z' --data-urlencode 'to=2026-10-22T00:00:00Z' \
  --data-urlencode 'source=sensor-a' --data-urlencode 'limit=10' http://127.0.0.1:8000/v1/events
curl -G -H "X-API-Key: $EVENT_ANALYST_KEY" \
  --data-urlencode 'from=2026-10-21T00:00:00Z' --data-urlencode 'to=2026-10-22T00:00:00Z' \
  --data-urlencode 'bucket=hour' http://127.0.0.1:8000/v1/analysis/timeseries
curl -G -H "X-API-Key: $EVENT_ANALYST_KEY" \
  --data-urlencode 'from=2026-10-21T00:00:00Z' --data-urlencode 'to=2026-10-22T00:00:00Z' \
  --data-urlencode 'numeric_field=value' http://127.0.0.1:8000/v1/analysis/summary
```

Resumen: `total_events`, `unique_sources`, `unique_types`, `first_occurred_at`, `last_occurred_at`, `by_type`, `by_source`, `numeric_summary`, `window`, `filters`. `numeric_summary` contiene `field`, `samples`, `min`, `max`, `avg`: solo cuenta valores JSON numéricos, nunca booleanos ni cadenas numéricas. Sin muestras: `samples=0`, estadísticas null. Sin eventos: fechas null y agrupaciones vacías. Los valores estadísticos se devuelven como float64; no están destinados a cálculos monetarios de precisión arbitraria. Listado y resumen usan snapshots repetibles para coherencia entre sus consultas.

## Errores y correlación

```json
{"error":{"code":"DUPLICATE","message":"Event already exists.","request_id":"req-123"}}
```

| HTTP | Código | Mensaje fijo |
|---|---|---|
| 401 | `UNAUTHENTICATED` | Authentication required. |
| 403 | `FORBIDDEN` | Access denied. |
| 404 | `NOT_FOUND` | Resource not found. |
| 405 | `INVALID_INPUT` | Invalid request. |
| 409 | `DUPLICATE` | Event already exists. |
| 422 | `INVALID_INPUT` | Invalid request input. |
| 503 | `DEPENDENCY_UNAVAILABLE` | Dependency unavailable. |
| 500 | `INTERNAL_ERROR` | Internal server error. |

Se normalizan JSON inválido, content-type incorrecto, cuerpo mayor de 4 MiB y parámetros desconocidos a 422. Métodos no admitidos devuelven 405 y conservan la cabecera `Allow`. No se reflejan valores de entrada ni errores SQL. Si coinciden varios fallos, no se garantiza prioridad salvo que se autentica antes de acceder al repositorio.

Todas las respuestas incluyen `X-Request-ID`. Se acepta una única cabecera con patrón `[A-Za-z0-9][A-Za-z0-9._-]{0,127}`; de lo contrario se genera UUID v4. Los errores registran código, estado, plantilla de ruta e ID; nunca payload, claves, DSN ni URL con valores del cliente. TLS, rate limiting y límites de conexión del proxy deben configurarse en el despliegue.
