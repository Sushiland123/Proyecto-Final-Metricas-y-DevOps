# Guía para agentes

## Commits y PR

- Un PR por issue: no agrupar requisitos distintos. Usar `.github/pull_request_template.md`; `Closes #N` solo si se cumplen todos los criterios, `Refs #N` si es parcial. Evidencia en `docs/evidencia/<ID>/`.
- Antes de crear commits, ejecutar `npm ci` (Node.js >=18): instala Husky y activa `.husky/_` como `core.hooksPath`. El `pre-commit` comprueba `cargo fmt --all -- --check`; corregir con `cargo fmt --all`, no saltarlo con `HUSKY=0` ni `--no-verify`. Node no es parte del runtime.
- Los PR deben recibir revisión de CodeRabbit (`.coderabbit.yaml`), además de revisión humana y CI. Si se omite la revisión, solicitarla con un comentario `@coderabbitai review`; un check `skipped` no es revisión. Resolver hallazgos o justificar desacuerdos antes de integrar.

## Aplicación y datos

- Hay un solo crate Rust; `src/main.rs` conecta `Settings`, `PostgresRepository` y `EventService`. `serve`, `process`, `drain` y `migrate` usan el mismo binario; `serve` no ejecuta el procesador ni aplica migraciones.
- El flujo genérico `api → service → repository` no es el escenario ESC3: `/v1/events` no alimenta su inbox ni contadores. ESC3 usa `/v1/readings`, `/v1/devices` y `src/scenario/`; consultar `docs/escenario.md` antes de cambiar reglas.
- La aplicación no carga `.env`: exportar el entorno explícitamente. Seguir el orden del README: PostgreSQL disponible → `cargo run --locked -- migrate` → API y procesador en procesos separados.
- SQLx embebe `migrations/` al compilar (`build.rs` detecta cambios); añadir migraciones SQL nuevas, no reescribir las aplicadas. `alembic/` es legado Python: no ejecutar ambos gestores sobre la misma BD.
- OpenAPI se genera desde Utoipa: `cargo run --locked -- openapi > openapi.json` no necesita BD. No mantener un contrato JSON paralelo a mano.

## Verificación

Usar el toolchain de `rust-toolchain.toml` y `--locked`. La secuencia del job `quality` es:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps
```

- Prueba enfocada: `cargo test --locked --test http <filtro>`. PostgreSQL y ESC3 están en el target `postgres`, no en un target independiente `scenario3`.
- `cargo test --locked --all-targets` no ejecuta los tests `ignored` de PostgreSQL. Con PostgreSQL 16 y `TEST_DATABASE_URL` apuntando a una BD dedicada `test_*` o `*_test` (si existe `DATABASE_URL`, debe coincidir), ejecutar `cargo test --locked --test postgres -- --ignored --nocapture`. La ausencia de BD debe fallar, no contarse como aceptación.
- `verify_system` crea su propio contenedor/volumen en otro puerto y los conserva al terminar; seguir el README antes de ejecutarlo. Requiere primero `cargo build --locked --bin event-service`.
- Seguridad local (Docker requerido): `bash scripts/scan-secrets.sh` escanea el historial; `bash scripts/verify-image.sh <imagen>` valida una imagen ya construida y escribe reportes/SBOM en `artifacts/image/`. El runtime distroless no tiene shell.
- Los checks de aplicación son `quality`, `postgres`, `secrets` y `container`; Husky solo comprueba formato, no sustituye estos jobs.

## Infraestructura

- Para validación sin credenciales, desde `infra/terraform/`: `terraform fmt -check -recursive -diff ..`, `terraform init -backend=false`, `terraform validate -no-color`, `tflint --format compact`. Versiones de referencia en `.github/workflows/terraform.yml`.
- El plan cloud se omite en forks; no confundirlo con un plan aprobado. El apply usa `main` y el environment `azure-infra`; no eludir su aprobación.
- Cambiar cloud-init o la versión K3s puede reemplazar la VM y perder volúmenes `local-path`. Consultar `docs/infra/README.md` antes de tocarlo; no versionar state, kubeconfig ni tfvars reales.
