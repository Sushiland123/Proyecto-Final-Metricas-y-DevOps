# Control de acceso y autorización — Escenario 3 (COM-05)

Este documento detalla el modelo de control de acceso, segregación de roles y aislamiento por origen implementado para el **Escenario 3: Ingesta y Análisis de Eventos**, en cumplimiento de **COM-05**.

---

## 1. Modelo de Actores y Roles

La plataforma define dos roles principales y un mecanismo de restricción de ámbito por origen de datos:

1. **`producer` (Productor de eventos):** Responsable exclusivo de la emisión de telemetría e ingesta de datos (`events:create`). No tiene autorización para inspeccionar eventos de otros productores ni ejecutar consultas o análisis agregados (`events:read`, `analysis:read`).
2. **`analyst` (Analista / Consumidor de analítica):** Autorizado para realizar exploración individual (`GET /v1/events/{id}`), consultas paginadas (`GET /v1/events`), conteos dimensionales (`GET /v1/analysis/counts`), series de tiempo (`GET /v1/analysis/timeseries`) y resúmenes estadísticos (`GET /v1/analysis/summary`). No tiene autorización para crear ni modificar eventos (`events:create`).
3. **`operator` (Sonda de infraestructura / K3s):** No requiere autenticación para inspeccionar la salud técnica del sistema (`/health/live`, `/health/ready`).

### Matriz de Control de Acceso (Operación vs Rol)

| Endpoint / Operación | Permiso requerido | Rol `producer` (Global) | Rol `producer` (Source Scoped) | Rol `analyst` | Sin clave / Clave inválida |
|---|---|---|---|---|---|
| `POST /v1/events` | `events:create` | **Permitida** | **Permitida si `source ∈ allowed_sources`**; de lo contrario **403 Forbidden** | 403 Forbidden | 401 Unauthorized |
| `POST /v1/events/batch` | `events:create` | **Permitida** | **Permitida si todos los `source ∈ allowed_sources`**; de lo contrario **403 Forbidden** | 403 Forbidden | 401 Unauthorized |
| `GET /v1/events/{id}` | `events:read` | 403 Forbidden | 403 Forbidden | **Permitida** | 401 Unauthorized |
| `GET /v1/events` | `events:read` | 403 Forbidden | 403 Forbidden | **Permitida** | 401 Unauthorized |
| `GET /v1/analysis/counts` | `analysis:read` | 403 Forbidden | 403 Forbidden | **Permitida** | 401 Unauthorized |
| `GET /v1/analysis/timeseries` | `analysis:read` | 403 Forbidden | 403 Forbidden | **Permitida** | 401 Unauthorized |
| `GET /v1/analysis/summary` | `analysis:read` | 403 Forbidden | 403 Forbidden | **Permitida** | 401 Unauthorized |
| `GET /health/live` | Público | **Permitida** | **Permitida** | **Permitida** | **Permitida** |
| `GET /health/ready` | Público | **Permitida** | **Permitida** | **Permitida** | **Permitida** |

---

## 2. Aislamiento lateral por fuente (Source Scoping)

Para prevenir el escalamiento lateral (donde un sensor o sistema externo suplanta a otro origen de telemetría), la plataforma admite la variable `EVENT_PRODUCER_SOURCE_KEYS`:
- Formato: `fuente1:clave1,fuente2:clave2`.
- Si un productor autentica con la clave de `sensor-a`, su objeto `Principal` queda restringido a `allowed_sources = {"sensor-a"}`.
- Cualquier intento de emitir eventos individuales o en lote hacia `sensor-b` es rechazado inmediatamente con `403 Forbidden` (`ensure_source_access`).
- La clave global de productor (`EVENT_PRODUCER_KEY`) mantiene `allowed_sources = None` para sistemas autorizados a emitir multi-fuente.

---

## 3. Principios de seguridad y falla cerrada

- **Doble verificación (Defensa en profundidad):** La autenticación y el rol se validan en la cabecera HTTP mediante dependencias FastAPI (`require_producer`, `require_analyst`). Adicionalmente, cada función de servicio (`create_event`, `create_events_batch`, `get_event`, `list_events`, `count_events`, `analyze_timeseries`, `analyze_summary`) vuelve a invocar `ensure_permission` y `ensure_source_access` antes de acceder al repositorio.
- **Falla cerrada (Fail-Safe Defaults):** Si alguna de las claves requeridas no está definida en el entorno, contiene solo espacios en blanco, ambas claves principales son idénticas, o existen colisiones entre claves de fuente y claves globales, el módulo de autenticación invalida la configuración y **rechaza toda petición protegida con 401 Unauthorized**.
- **Comparación en tiempo constante:** Todas las verificaciones de credenciales emplean `hmac.compare_digest` para evitar ataques de temporización (timing attacks).
- **Protección de secretos en logs y respuestas:** Los valores de las claves nunca se imprimen en logs, respuestas de error ni volcados de traceback (`Settings` utiliza `SecretStr` y `repr=False`).

---

## 4. Evidencia verificable

Las pruebas automatizadas en `tests/test_auth.py` y `tests/test_acceptance.py` demuestran de forma exhaustiva:
1. Acceso autorizado para productores y analistas en todas las rutas protegidas.
2. Prevención de escalamiento vertical (403 cuando un productor intenta consultar analítica o un analista intenta emitir eventos).
3. Prevención de escalamiento lateral (403 cuando un productor restringido intenta emitir hacia una fuente no asignada).
4. Rechazo de accesos no autenticados o con claves mal formadas (401).
5. Falla cerrada ante configuraciones incompletas o colisiones de claves.
6. Ausencia de secretos en respuestas HTTP y registros de logs.
