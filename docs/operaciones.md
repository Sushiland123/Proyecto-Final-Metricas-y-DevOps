# Runbook operativo · Event Service

## Alcance y responsabilidades

El servicio es una aplicación íntegramente Rust distribuida como binario o imagen no-root. PostgreSQL mantiene la autoridad de consistencia y durabilidad. Este runbook describe los controles de operación del software; no acredita una plataforma Azure/K3s desplegada, alta disponibilidad ni un SLO medido.

| Componente | Responsabilidad operativa |
|---|---|
| API | Autenticación, validación, casos de uso, errores públicos y correlación |
| PostgreSQL | Transacciones, restricciones, consultas, almacenamiento y recuperación de datos |
| Job de migración | Aplicar el esquema versionado antes de habilitar tráfico a la versión nueva |
| Proxy / ingress | TLS, rate limiting, límites de conexiones y timeouts HTTP |
| Plataforma | Supervisión de procesos, secretos, backups, almacenamiento y recursos |

## Controles previos a una entrega

1. Asociar el commit a resultados aprobados de formato, Clippy, pruebas, rustdoc y build del contenedor.
2. Ejecutar aceptación contra PostgreSQL real; no considerar suficiente la suite que excluye integración.
3. Revisar cambios de esquema y compatibilidad de lectura/escritura durante el rollout. Una migración aplicada no se modifica: se publica otra versión.
4. Verificar disponibilidad y restauración del backup antes de adoptar una base existente o realizar un cambio incompatible.
5. Ejecutar `event-service migrate` con una identidad autorizada para DDL. La identidad del servicio debe limitarse a los permisos de datos necesarios; los roles PostgreSQL concretos deben aprovisionarse en el despliegue.
6. Desplegar la imagen identificada por digest, validar readiness y ejecutar una prueba funcional antes de promover tráfico.
7. Conservar commit, digest, versiones SQLx y resultados de verificación como evidencia de entrega.

## Salud y diagnóstico inicial

| Señal | Interpretación | Acción |
|---|---|---|
| Liveness 200, readiness 200 | Proceso y acceso básico a DB disponibles | Verificar un caso de uso; `SELECT 1` no acredita permisos de datos ni rendimiento |
| Liveness 200, readiness 503 | Proceso activo sin acceso operativo a DB | Revisar conectividad, DSN, credenciales, saturación del pool y estado PostgreSQL |
| Liveness no responde | Proceso, listener, red o plataforma afectados | Revisar supervisor, recursos, puerto y logs de inicio |
| Datos 503 con readiness 200 | Fallo de consulta, permisos, esquema, timeout o capacidad | Correlacionar Request-ID; inspeccionar DB mediante herramientas administrativas seguras |
| 401 generalizado | Credenciales ausentes, ambiguas o configuración de fuentes inválida | Verificar la referencia a secretos sin imprimir sus valores; corregir y reemplazar procesos |
| 403 | Identidad autenticada sin rol o fuente autorizada | Revisar la asignación de permisos; no ampliar privilegios como mitigación automática |
| 409 | Clave natural ya persistida o duplicado en lote | Consultar el resultado; no reintentar el mismo lote de forma indefinida |

Los errores públicos no incluyen diagnósticos internos. Usar el Request-ID para correlación y acceder a diagnósticos administrativos únicamente por canales autorizados. No habilitar logs SQL que registren parámetros, DSNs o payloads sensibles.

## Timeouts, capacidad y presión de carga

El pool se comparte por proceso y admite hasta 10 conexiones. Los límites configurables acotan adquisición/conexión y cada sentencia; los reintentos de lectura agregan espera y backoff. No existe un deadline HTTP global dentro del servicio.

Dimensionar el pool por réplica considerando el límite total de conexiones PostgreSQL y las herramientas administrativas. No aumentar réplicas o conexiones sin comprobar capacidad de CPU, I/O y memoria de DB. Las consultas amplias y OFFSET elevados requieren análisis con `EXPLAIN (ANALYZE, BUFFERS)` sobre datos representativos y fuera de tráfico crítico.

Antes de fijar un SLO, medir al menos latencia p50/p95/p99, throughput, proporción de 5xx, tiempos de adquisición, utilización del pool, CPU/RSS y duración de consultas. El lenguaje de implementación no constituye evidencia de capacidad ni ahorro económico.

## Recuperación ante fallos

- **Interrupción transitoria de DB:** mantener liveness; `/health/ready` informa 503 pero no modifica el enrutamiento por sí mismo. El proxy o la plataforma debe configurar una sonda y usar su resultado para retirar la instancia del tráfico; Compose no incluye esa integración. Las lecturas transitorias tienen reintentos limitados. Al recuperar DB, el pool descarta conexiones inválidas y abre conexiones nuevas.
- **Resultado de escritura incierto:** una desconexión durante COMMIT puede ocurrir después de persistir. Verificar el evento por consulta; no asumir que un 503 implica ausencia de escritura. Un reenvío con la misma clave natural puede devolver 409.
- **Fallo de migración:** detener la promoción, conservar diagnósticos y revisar esquema/checksums. No registrar manualmente una migración como aplicada para omitir una incompatibilidad.
- **Rollback de aplicación:** solo desplegar el binario anterior si su contrato de datos es compatible con el esquema vigente. No ejecutar un downgrade destructivo como parte automática del rollback.
- **Pérdida de almacenamiento:** restaurar desde backup y verificar integridad antes de habilitar tráfico. Un volumen persistente o un reinicio satisfactorio no equivalen a un backup probado.

## Secretos y exposición de red

La configuración se fija al iniciar. Para rotar claves, actualizar el secreto de plataforma y reemplazar los procesos de manera controlada; no se ofrece recarga dinámica ni solapamiento de claves globales. Coordinar el cambio con productores y consumidores.

El proceso nativo escucha en loopback de forma predeterminada. La imagen escucha en `0.0.0.0:8000` para permitir su publicación por la plataforma. Exponer la API detrás de TLS y no publicar PostgreSQL a Internet. Nunca incluir `.env`, credenciales o cadenas de conexión en la imagen o artefactos.

## Experimento reproducible

Con la configuración de prueba dedicada del README:

```bash
cargo build --locked --bin event-service
cargo run --locked --example verify_system
```

El programa crea un contenedor propio, verifica recuperación sin cambiar el PID API, reinicia ambos componentes y compara el documento persistido. También comprueba respuestas y logs ante marcadores sensibles. Los contenedores quedan detenidos y sus volúmenes conservados; registrar sus nombres y eliminarlos explícitamente cuando concluya la inspección.

La evidencia local y la ejecución CI deben enlazarse a su revisión concreta. Estas pruebas cubren fallos de proceso/conectividad con almacenamiento conservado, no un desastre de zona ni garantías HA.
