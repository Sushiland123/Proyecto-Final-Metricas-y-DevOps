# Desarrollo y arquitectura del código Rust

## Dependencias y límites

```text
main (composition root) ── configura Settings, pool y EventService
api (Axum) ── autentica, extrae DTO, genera OpenAPI, normaliza errores
service ── valida, autoriza y ejecuta casos de uso
repository::EventRepository ── puerto asíncrono de persistencia
postgres::PostgresRepository ── adaptador SQLx / PostgreSQL
domain ── DTO, enums, validación y respuestas tipadas
auth / config / error ── políticas y soporte transversal
```

**Patrones empleados, con propósito:**
- **Repository + inversión de dependencias:** `EventService` recibe `Arc<dyn EventRepository>`; las pruebas sustituyen PostgreSQL sin modificar casos de uso. No añadimos una interfaz por cada clase.
- **Service Layer:** un lugar para autorización, validación y deduplicación intra-lote. Acceso directo a casos de uso no permite saltarse RBAC.
- **Dependency Injection / Composition Root:** solo `main.rs` decide el adaptador real y comparte configuración/pool; no hay contenedor DI ni estado global mutable.
- **Unit of Work / RAII:** transacción SQLx por lote; rollback explícito en fallo de inserción y rollback al liberar transacciones no confirmadas. Listado/resumen leen un snapshot `REPEATABLE READ READ ONLY`.
- **DTO y enums:** contratos Serde/Utoipa, `Dimension` y `Bucket` evitan strings arbitrarios como identificadores SQL.

Los contratos de `domain.rs` comparten metadatos de serialización, persistencia y OpenAPI para mantener una única definición verificable. Esta decisión reduce conversiones entre DTO equivalentes, a cambio de acoplar esos tipos a Serde, SQLx y Utoipa. Los casos de uso se mantienen independientes del adaptador concreto de persistencia. `error.rs` centraliza la semántica y traducción HTTP; separar modelos adicionales cuando sus invariantes o ciclos de evolución diverjan.

## Reglas de mantenimiento

1. Añadir regla/contrato a `domain.rs` y caso de uso a `service.rs`; mantener controladores pequeños.
2. Parametrizar todo dato SQL; solo interpolar nombres constantes provenientes de enums.
3. Nunca `unwrap`/`expect` en producción; Clippy los prohíbe. Las pruebas pueden usarlos para aserciones.
4. No imprimir excepciones de drivers, secretos o datos recibidos. `Settings` no implementa `Debug` ni `Serialize`.
5. Mantener módulos cohesivos; no agregar factories, singletons o microservicios sin un problema concreto.
6. Una vez aplicada una migración, no editar su contenido: añadir otra SQL versionada. SQLx comprueba checksums y serializa migraciones con lock.
7. Cambiar contrato junto con anotaciones Utoipa, guía API, matriz de reglas y pruebas.

## Pruebas

| Archivo | Responsabilidad |
|---|---|
| `tests/domain.rs` | Fechas, identificadores, tamaño UTF-8, lotes y límites de consulta |
| `tests/auth.rs` | RBAC, fuentes, configuración ambigua y límites de configuración |
| `tests/service.rs` | Permisos antes de acceder al repositorio y recuperación con doble de prueba |
| `tests/http.rs` | Contrato HTTP, errores, extracción, request IDs y OpenAPI |
| `tests/postgres.rs` | Persistencia real, concurrencia, rollback, filtros, agregados y migraciones |
| `examples/verify_system.rs` | Experimento real con procesos y contenedor propios; no un mock |

`cargo test --locked --all-targets` ejecuta las pruebas sin DB y compila el experimento. PostgreSQL requiere ejecución explícita con `--test postgres -- --ignored`; faltar configuración es un error. Los nombres de BD deben empezar por `test_` o terminar por `_test`; cada prueba limpia solo su fuente UUID. Si una prueba falla antes de limpiar, sus filas aisladas pueden permanecer para diagnóstico. No usar una base compartida de producción.

## Herramientas y CI

Rust fijado por `rust-toolchain.toml`, dependencias por `Cargo.lock`. Ejecutar fmt, Clippy con `-D warnings`, pruebas y rustdoc siguiendo el README. El workflow `acceptance.yml` separa calidad, PostgreSQL/experimentos y build del contenedor no-root. Publica el contrato, rustdoc y resultados de aceptación como artefactos. No publica imágenes en un registro ni aplica infraestructura cloud automáticamente.

`cargo run --locked -- openapi` no necesita DB. `cargo doc --locked --no-deps` documenta los módulos públicos. Consultas SQL dinámicas se verifican contra PostgreSQL en integración, no mediante conexión a DB durante compilación.
