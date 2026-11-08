# DEV-03 · Integración continua y protección de la rama

[Issue #9](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/9). Criterios: todo PR ejecuta las validaciones, un check fallido impide el merge, los checks están declarados en la protección de la rama y los reportes quedan disponibles.

## Jobs de `acceptance.yml`

| Job (nombre del check) | Valida | Artefacto |
|---|---|---|
| `quality` | Formato, Clippy, pruebas, rustdoc y contrato OpenAPI | `rust-api-and-code-docs` |
| `postgres` | Aceptación contra PostgreSQL real y experimento de recuperación | `rust-postgres-evidence` |
| `secrets` | Secretos en todo el historial de Git | — |
| `container` | Build de la imagen, endurecimiento, vulnerabilidades y SBOM | `container-security-evidence` |

Las validaciones de infraestructura están en `terraform.yml` (DEV-08). Las dependencias se cachean con `rust-cache` y la caché de Buildx. Los jobs `secrets` y `container` se reproducen en local con los scripts de [`scripts/`](../../../scripts).

## Protección de `main`

Estado leído de la API de GitHub el 2026-11-08: protección activa con los checks requeridos `quality`, `postgres` y `container`, con `enforcement_level: non_admins`. [`CODEOWNERS`](../../../.github/CODEOWNERS) asigna la revisión de todos los cambios al dueño del repositorio.

Falta agregar `secrets` en **Settings → Branches**. Ese nombre solo se puede seleccionar después de que el job haya corrido una vez en este repositorio.

No hay que marcar los checks de Terraform como requeridos: solo corren si el PR toca `infra/**`, y en los demás PR quedarían esperando para siempre.

## Pendiente

- [ ] Agregar `secrets` a los checks requeridos.
- [ ] Demostrar con un PR de prueba que un check fallido bloquea el merge.
- [ ] Enlazar las ejecuciones reales.

## Limitaciones

Los PR desde un fork no reciben OIDC ni escritura en el registro. Esto no afecta a estas validaciones, porque ninguna necesita credenciales.

Con `non_admins`, un administrador puede integrar aunque los checks fallen, de modo que el bloqueo solo es total para los demás integrantes.
