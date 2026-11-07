# Control de acceso y segregación · Rust / COM-05

API keys en `X-API-Key`; nunca en query o body. La autenticación se resuelve en el extractor `Principal` de `api.rs`; el servicio vuelve a exigir el rol y el alcance antes de cualquier acceso al repositorio.

| Rol | Permisos |
|---|---|
| producer global | Ingesta individual y lote de cualquier fuente |
| producer restringido | Ingesta solo de las fuentes asignadas |
| analyst | Lectura de eventos y análisis; no escritura |

`EVENT_PRODUCER_KEY` y `EVENT_ANALYST_KEY` deben existir, ser no vacías y distintas. Configuración parcial/ambigua falla cerrado con 401 incluso si una clave recibida coincide. Fuente no autorizada o rol insuficiente: 403.

`EVENT_PRODUCER_SOURCE_KEYS=sensor-a:clave,sensor-b:otra` (también acepta `=`). Fuentes simples ASCII, sin duplicados; claves no vacías y sin colisión con claves globales. Una misma clave restringida puede habilitar varias fuentes. Lotes verifican todas las fuentes antes de escribir para impedir cambios parciales. Cabeceras API-key duplicadas se rechazan.

La coma separa entradas y no puede formar parte de una clave. El parser da prioridad a `:`: con el formato alternativo `fuente=clave`, una clave que contenga `:` invalida la configuración completa. Usar claves aleatorias hexadecimales evita estas ambigüedades; el formato `fuente:clave` permite `:` dentro de la clave después del primer separador.

Se comparan digests SHA-256 de longitud fija mediante `subtle::ConstantTimeEq`. Esta es una comparación de secretos de alta entropía, no un sistema de almacenamiento de contraseñas humanas. Generar claves aleatorias, transportarlas bajo TLS y guardarlas como secretos del despliegue. La comparación no convierte toda la petición HTTP en tiempo constante.

`Settings` no implementa Debug/Serialize. Configuración leída al arrancar; rotación requiere reinicio. No existe login humano, JWT, revocación dinámica ni rate limiting interno: si el escenario crece, reevaluar OIDC/gestor de secretos y proxy.

Pruebas: `tests/auth.rs`, `tests/service.rs::service_rechecks_permissions_before_repository_access`, `tests/http.rs` y pruebas reales de lote en `tests/postgres.rs`. Ejecutar según README.
