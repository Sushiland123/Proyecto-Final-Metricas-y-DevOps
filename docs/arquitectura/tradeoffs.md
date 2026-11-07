# Trade-offs · Rust / Escenario 3 / COM-06

| Eje | Decisión y alternativa | Coste/riesgo y revisión |
|---|---|---|
| Plataforma de ejecución | Implementación íntegramente Rust con Axum/Tokio/SQLx; tipos estáticos, I/O asíncrono y distribución como binario | Compilación y mantenimiento más exigentes. Latencia p95, throughput, CPU, RSS y coste deben evaluarse con la misma carga antes de establecer mejoras cuantitativas. |
| Consistencia | 201 solo tras COMMIT, lote ACID y unicidad en PostgreSQL; sin cola | DB caída implica 503. Desacoplar solo cuando requisitos de ingestión/disponibilidad lo justifiquen; no se inventa un umbral de escrituras sin medición. |
| Lecturas | SQL bajo demanda; snapshots repetibles para listado/resumen | Agregados amplios pueden ser caros; índices no garantizan milisegundos sobre millones de filas. Medir EXPLAIN ANALYZE antes de vistas materializadas, particiones o motor columnar. |
| Disponibilidad | API sin estado + una DB con volumen; no HA | Punto único de fallo. Si SLA/RTO exige continuidad, evaluar réplica y backup/restore probado. Un volumen no es un backup. |
| Complejidad | Monolito modular, Repository port y DI explícita; sin microservicios ni framework DI | Se escala ingesta/análisis juntos. Separar solo por ciclos de entrega o perfiles de carga demostrados. |
| Concurrencia | I/O async y pool máximo 10, sin bloquear hilos en backoff | El pool puede saturarse; ajustar con métricas y capacidad PostgreSQL. Rust no elimina límites de disco/red/DB. |
| Seguridad | API keys con RBAC y alcance por fuente; sin IdP externo | Rotación manual/reinicio. OIDC si aparecen usuarios humanos/federación; rate limiting y TLS en proxy. |
| Coste | Objetivo de curso Azure < US$100, evaluar VM/K3s y apagar recursos sin uso | Es un objetivo pendiente, no un coste demostrado. Menor RSS no abarata una VM ya contratada si no permite redimensionarla. |
| Migraciones | SQLx embebido, ejecución explícita antes del deploy | Mantener checksums; adopción conservadora del esquema antiguo. Backup y ensayo obligatorio antes de transición con datos reales. |

El servicio, sus pruebas y las herramientas de verificación están implementados en Rust; no existe un runtime de aplicación secundario. El esquema relacional y el contrato HTTP permanecen estables. La decisión de plataforma, sus límites y los criterios de aceptación se detallan en [ADR-0004](../adr/0004-rust.md).
