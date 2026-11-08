# DEV-08 · Validación de Terraform en GitHub Actions

[Issue #14](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/14) · [PR #40](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/pull/40). Criterios:

- todo PR de infraestructura ejecuta validación y plan;
- el plan es visible antes del merge;
- el apply requiere aprobación y no corre en PRs;
- no hay credenciales de Azure en el repositorio.

## Workflows

| Workflow | Disparador | Qué hace |
|---|---|---|
| [`terraform.yml`](../../../.github/workflows/terraform.yml) | PR que toca `infra/**` | `fmt -check`, `init -backend=false`, `validate`, `tflint` y speculative plan con `id-eventos-plan`. El plan se publica en el resumen del job y como artefacto `terraform-plan-pr-<n>` |
| `terraform.yml` | push a `main` o manual | `plan -out=tfplan` y `apply tfplan` en el environment `azure-infra`, que exige la aprobación de un revisor antes de entregar el token de `id-eventos-apply` |
| [`terraform-drift.yml`](../../../.github/workflows/terraform-drift.yml) | diario, 08:00 | `plan -detailed-exitcode` de solo lectura; falla si Azure difiere del código |

Las identidades usan OIDC, así que no hay credenciales de Azure en el repositorio. Las variables guardan IDs de cliente, tenant y suscripción. La IP de administración y los correos van como *secrets* y como variables `sensitive` de Terraform, solo por privacidad: el repositorio es público. El job de apply tiene la condición `github.event_name != 'pull_request'`, de modo que ningún PR puede aplicar.

## Resultado obtenido · 2026-11-08

- **Local, equivalente al job `validate`:** `terraform fmt -check -recursive` sin diferencias y `terraform validate` exitoso con AzureRM 4.40.0. Los YAML de los workflows se analizaron sin errores. `tflint` no se ejecutó en local.
- **En GitHub, pendiente:** las ejecuciones del PR #40 están en `action_required`, porque el PR viene de un fork y un mantenedor debe pulsar *Approve and run*:
  - [Terraform sobre `7c52619`](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/actions/runs/42446613442);
  - [Terraform sobre `4bde714`](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/actions/runs/42445426635).

  Los forks no reciben token OIDC, así que en ese PR el plan se omite con un aviso. El speculative plan corre en PRs desde una rama del propio repositorio, una vez configuradas las variables (README, *Configuración del repositorio*).

Agregar aquí, cuando existan:

- [ ] Enlace a la ejecución de `fmt`/`validate`/`tflint` del PR aprobada.
- [ ] Enlace a un speculative plan de un PR del mismo repositorio, con su artefacto.
- [ ] Enlace a un apply en `main` que muestre la espera de aprobación de `azure-infra`.
- [ ] Enlace a una ejecución de `terraform-drift.yml` con `No changes`.

## Limitaciones

- Hasta que el repositorio tenga los environments y las variables, solo corre `validate`.
- Los cron de GitHub pueden retrasarse algunos minutos.
