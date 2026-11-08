# DEV-07 · Estado remoto de Terraform con acceso protegido

[Issue #13](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/13) · [PR #40](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/pull/40). Criterios:

- el estado no está en Git y el backend está documentado;
- `init` funciona desde cero;
- el acceso usa identidad/RBAC con mínimo privilegio;
- existe un procedimiento de recuperación y de bloqueo.

## Configuración

- Backend: [`versions.tf`](../../../infra/terraform/versions.tf) (`backend "azurerm" {}`) y [`backend.hcl`](../../../infra/terraform/backend.hcl), con `use_azuread_auth = true` y sin claves.
- Bootstrap reproducible: [`infra/bootstrap/bootstrap.sh`](../../../infra/bootstrap/bootstrap.sh), idempotente.
- Procedimiento, bloqueo y recuperación: [docs/infra/README.md](../../infra/README.md#estado-remoto--dev-07).
- `.gitignore` excluye `*.tfstate*`, `*.tfvars`, `tfplan` y `infra/bootstrap/bootstrap.env`.

## Resultado obtenido · 2026-11-08

Storage del state (`az storage account show` y `blob-service-properties show`):

```text
SharedKey    PublicBlob    Tls     HttpsOnly    Region
False        False         TLS1_2  True         westus

Versionado    RetencionBlobs    RetencionContainers
True          14                14
```

Accesos (`az role assignment list`, suscripción redactada):

```text
id-eventos-plan   Reader                         /subscriptions/<sub>
id-eventos-plan   Storage Blob Data Contributor  .../storageAccounts/sttfeventosb97e31
id-eventos-apply  Contributor                    /subscriptions/<sub>
id-eventos-apply  Storage Blob Data Contributor  .../storageAccounts/sttfeventosb97e31
```

Ninguna identidad tiene secretos: GitHub Actions obtiene tokens por OIDC con credenciales federadas.

```text
repo:marchelo23/Proyecto-Final-Metricas-y-DevOps:pull_request              (id-eventos-plan)
repo:marchelo23/Proyecto-Final-Metricas-y-DevOps:environment:azure-infra   (id-eventos-apply)
```

Las credenciales también existen con el formato de IDs inmutables. El bootstrap actualizado agrega `environment:azure-ops`, para `id-eventos-ops` (energía de la VM) y para `id-eventos-plan` (drift programado). Esta evidencia se completará con la salida de `az identity federated-credential list` cuando se ejecute.

**Bloqueo.** Cada `plan` y `apply` muestra `Acquiring state lock` y toma un lease del blob. Un plan cortado por la red dejó el lease tomado: el siguiente plan falló con `state blob is already locked`, que es el comportamiento esperado ante una ejecución concurrente. Se confirmó que no había otra ejecución en curso (sin procesos locales; CI sin configurar) y se liberó con `az storage blob lease break`, sin usar `-lock=false`.

**Recuperación (simulacro sin tocar el state real).** Se descargaron versiones anteriores del blob con `--version-id` y se comparó su contenido con el actual:

```text
versión                        serial  recursos  VM (vmId)
2026-11-08T09:09:42.8223670Z   25      9         80faf14c…   ← actual
2026-11-08T09:01:35.7190709Z   24      9         80faf14c…
2026-11-08T08:59:51.1288356Z   23      9         61c232b5…   ← VM anterior, antes del reemplazo
2026-11-08T08:47:49.6135345Z   22      9         61c232b5…
2026-11-08T08:46:42.5709003Z   21      5         —            ← antes de crear la VM
```

Todas comparten el mismo `lineage`. Cualquiera puede restaurarse con `terraform state push -force` siguiendo el procedimiento del README. Las copias descargadas se eliminaron al terminar.

## Limitaciones

- El Storage acepta conexiones públicas porque los runners de GitHub no tienen IP fija. La protección es de identidad (Entra ID + RBAC, sin access keys).
- `id-eventos-plan` necesita Storage Blob Data Contributor para tomar el lease, así que técnicamente podría escribir el state. Solo la usan workflows de PR y el environment `azure-ops`, limitado a `main`.
- El Storage está en `westus` (creado antes de mover la plataforma); el state no depende de la región de la VM.
