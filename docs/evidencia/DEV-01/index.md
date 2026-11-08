# DEV-01 · Versionar aplicación, infraestructura y configuración no secreta

[Issue #7](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/7). Criterios: todo está en el repositorio sin secretos en el historial, `main` no acepta pushes directos, hay historial de PR revisados y el escaneo de secretos corre en cada PR.

## Convención

- Ramas: `tipo/ID-descripcion`, por ejemplo `infra/DEV-06-terraform-azure` o `ci/DEV-01-02-03-secretos-imagen`.
- Commits: `tipo(ID): descripción`.
- Plantilla de PR: [`.github/pull_request_template.md`](../../../.github/pull_request_template.md).
- Se excluyen de Git los secretos, el state de Terraform, los kubeconfig y los artefactos: [`.gitignore`](../../../.gitignore).

## Escaneo de secretos

```bash
bash scripts/scan-secrets.sh
```

Revisa el historial completo de todas las ramas con gitleaks, y el job `secrets` de [`acceptance.yml`](../../../.github/workflows/acceptance.yml) lo ejecuta en cada PR y en cada push a `main`.

| Fecha | Resultado |
|---|---|
| 2026-11-08, sin excepciones | 2 hallazgos `generic-api-key` en `tests/test_persistence.py` (commit `86e922b`) |
| 2026-11-08, con [`.gitleaks.toml`](../../../.gitleaks.toml) | `no leaks found` |

Los dos hallazgos son claves ficticias de una prueba Python ya eliminada (`com03-test-producer-key` y `com03-test-analyst-key`). Nunca fueron credenciales. La excepción acepta exactamente esos dos valores, no el archivo ni la regla completa.

## Pendiente

- [ ] Enlazar la ejecución del job `secrets` en el PR.
- [ ] Agregar `secrets` a los checks requeridos de `main`. La protección ya está activa; el detalle está en [DEV-03](../DEV-03/index.md).

## Limitaciones

gitleaks detecta patrones conocidos y texto de alta entropía. Un resultado limpio no prueba que no exista ningún secreto con un formato poco común.
