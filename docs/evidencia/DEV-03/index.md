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

## Protección de `main` (la configura un administrador)

**Settings → Branches → Add branch ruleset** o **Add rule**, para `main`:

1. Exigir pull request antes del merge, con al menos 1 aprobación.
2. Exigir checks correctos, marcando `quality`, `postgres`, `secrets` y `container`.
3. Bloquear force pushes y borrado de la rama.
4. Aplicar la regla también a los administradores.

No hay que marcar los checks de Terraform como requeridos: solo corren si el PR toca `infra/**`, y en los demás PR quedarían esperando para siempre.

Los nombres `secrets` y `container` solo se pueden seleccionar después de que hayan corrido al menos una vez en este repositorio.

## Pendiente

- [ ] Configurar la protección.
- [ ] Demostrar con un PR de prueba que un check fallido bloquea el merge.
- [ ] Enlazar las ejecuciones reales.

## Limitaciones

Los PR desde un fork no reciben OIDC ni escritura en el registro. Esto no afecta a estas validaciones, porque ninguna necesita credenciales.
