# Contenedores y módulos Rust · COM-02

Tres unidades de ejecución: `event-service serve` (Axum/Tokio), `event-service process` (procesador ESC3) y PostgreSQL 16 con volumen. Comparten binario, no ciclo de vida. El servicio no se divide en microservicios. Diagrama específico y decisiones del inbox en [Escenario 3](../escenario.md).

```mermaid
flowchart LR
    producer["Productor"] -->|"POST /v1/events y /batch"| api
    analyst["Analista"] -->|"GET /v1/events y /analysis/*"| api
    operator["Operador / probes"] -->|"GET /health/live y /ready"| api
    subgraph process["event-service · Rust"]
        main["main.rs · composición, CLI y señales"] --> config["config.rs · entorno"]
        main --> api["api.rs · Axum y OpenAPI"]
        api --> auth["auth.rs · API keys y RBAC"]
        api --> service["service.rs · casos de uso"]
        service --> auth
        service --> domain["domain.rs · contratos y validación"]
        service --> port["repository.rs · EventRepository"]
        adapter["postgres.rs · adaptador SQLx"] -.->|"implementa"| port
        main --> adapter
        api --> errors["error.rs · respuestas seguras"]
    end
    adapter -->|"SQL async / ACID / agregados"| pg[("PostgreSQL 16 · events JSONB")]
    api --> scenario["scenario/http.rs · dispositivos y lecturas ESC3"]
    scenario --> inbox[("PostgreSQL · devices / readings / reception_receipts")]
    worker["event-service process · proceso separado"] --> inbox
    worker --> alerts[("PostgreSQL · business_alerts")]
```

| Módulo | Responsabilidad |
|---|---|
| `main.rs` | Construye configuración, adaptador y servicio; comandos `serve`, `migrate`, `openapi`; apagado ordenado |
| `api.rs` | Extracción HTTP, autenticación, rutas, sondas, OpenAPI y correlación; no hace SQL |
| `domain.rs` | Tipos de entrada/salida, enums, UTC y límites verificables |
| `service.rs` | Autoriza todos los casos de uso, valida entradas y deduplica dentro del lote |
| `repository.rs` | Puerto asíncrono intercambiable para persistencia |
| `postgres.rs`, `migrations/` | SQL parametrizado, pool compartido de máximo 10 conexiones, transacciones, snapshots y reintentos |
| `auth.rs` | Claves globales y por fuente; comparación constante de digests SHA-256 |
| `error.rs` | Enum semántico y traducción a JSON seguro |
| `config.rs` | Configuración de inicio sin Debug/Serialize para secretos |
| `scenario/` | Contrato ESC3, recepción durable, contadores, consultas y procesamiento transaccional con backlog |

La API genérica, ESC3 y readiness comparten un único pool de hasta 10 conexiones;
el procesador independiente utiliza otro de hasta 10. Dimensionar PostgreSQL por
**proceso y réplica**, no asumir que toda la plataforma usa 10 conexiones.
Se crean conexiones bajo demanda.

Contratos públicos en [API](../api.md); patrones y límites pragmáticos en [desarrollo](../desarrollo.md).
