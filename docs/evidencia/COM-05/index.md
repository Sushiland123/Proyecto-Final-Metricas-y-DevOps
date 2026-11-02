# COM-05 — Evidencia de controles de acceso y segregación (Escenario 3)

**Criterio:** Control de acceso apropiado para el Escenario 3: segregación de roles (`producer` para ingesta puntual/lote, `analyst` para consultas/análisis), rechazo de accesos no autenticados (401), prevención de escalamiento vertical (403), y aislamiento lateral por origen emisor (`EVENT_PRODUCER_SOURCE_KEYS`). Véase [issue #5](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/5).

## 1. Reproducción de la suite de autenticación y autorización

```bash
uv sync --locked
uv run --locked pytest tests/test_auth.py
```

## 2. Casos cubiertos y comprobados

| Caso de prueba | Condición probada | Resultado |
|---|---|---|
| Acceso autorizado por rol | Clave válida de `producer` en `POST /v1/events` y `POST /v1/events/batch`; clave de `analyst` en rutas `GET /v1/*`. | `200 OK` / `201 Created` |
| Escalamiento vertical denegado | Productor intentando leer analítica; analista intentando emitir eventos. | `403 Forbidden` |
| Escalamiento lateral denegado | Productor con clave asignada exclusivamente a `sensor-a` intentando emitir eventos hacia `sensor-b`. | `403 Forbidden` |
| Credencial ausente o incorrecta | Petición sin cabecera `X-API-Key` o con token desconocido. | `401 Unauthorized` |
| Configuración ambigua o incompleta | Claves faltantes, claves vacías o claves idénticas entre roles. | `401 Unauthorized` (Falla cerrado) |
| Protección de credenciales | Respuestas de error y logs sanitizados; los secretos nunca aparecen en texto plano. | `assert secret not in response.text + caplog.text` |
