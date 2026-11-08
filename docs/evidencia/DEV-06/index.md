# DEV-06 · Recursos principales de Azure con Terraform

[Issue #12](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/12) · [PR #40](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/pull/40). Criterios:

- `fmt` y `validate` pasan;
- el plan es revisable y no tiene destrucciones inesperadas;
- el apply se hace desde una suscripción autorizada;
- los recursos llevan tags de proyecto, ambiente y costo.

Código en [`infra/terraform/`](../../../infra/terraform/); decisiones y recursos en [docs/infra/README.md](../../infra/README.md).

## Reproducir

```bash
cd infra/terraform
export ARM_USE_CLI=true
terraform fmt -check -recursive .. && terraform validate
terraform init -backend-config=backend.hcl
terraform plan -out=tfplan      # revisar antes de aplicar
terraform apply tfplan
```

## Resultado obtenido · 2026-11-08

Suscripción Azure for Students del equipo, región `mexicocentral`, AzureRM `4.40.0`, Terraform `1.16.6`.

| Comprobación | Resultado |
|---|---|
| `terraform fmt -check -recursive` | Sin diferencias |
| `terraform validate` | `Success! The configuration is valid.` |
| Plan al corregir cloud-init (revisado antes de aplicar) | `Plan: 2 to add, 0 to change, 1 to destroy`: reemplazo de la VM por `custom_data`, sin tocar red ni IP |
| Plan al agregar tags (revisado antes de aplicar) | `Plan: 0 to add, 6 to change, 0 to destroy`: solo tags, en sitio |
| Plan después de aplicar | `No changes. Your infrastructure matches the configuration.` |

Un plan revisado detectó y evitó una destrucción inesperada: la primera versión del plan de tags quería recrear la subred. La causa era un `replace_triggered_by` demasiado amplio, que se corrigió antes de aplicar (ver *Incidentes* en el README).

Recursos administrados (`terraform state list`):

```text
data.azurerm_subscription.current
azurerm_consumption_budget_subscription.project[0]
azurerm_linux_virtual_machine.k3s
azurerm_network_interface.k3s
azurerm_network_security_group.k3s
azurerm_public_ip.k3s
azurerm_resource_group.main
azurerm_subnet.k3s
azurerm_subnet_network_security_group_association.k3s
azurerm_virtual_network.main
```

Recursos en Azure con sus tags (`az resource list -g rg-eventos-b97e31`):

```text
Recurso                    Environment    Cost_center            Project
nsg-eventos-b97e31-k3s     compartido     proyecto-catedra-2026  eventos
pip-eventos-b97e31-k3s     compartido     proyecto-catedra-2026  eventos
vnet-eventos-b97e31        compartido     proyecto-catedra-2026  eventos
nic-eventos-b97e31-k3s     compartido     proyecto-catedra-2026  eventos
vm-eventos-b97e31-k3s      compartido     proyecto-catedra-2026  eventos
osdisk-eventos-b97e31-k3s                                        eventos
```

VM: `Standard_B2s_v2`, zona `1`, `mexicocentral`, `ubuntu-24_04-lts`, autenticación por contraseña deshabilitada.

Outputs (`terraform output`):

```text
public_fqdn           = "eventos-b97e31.mexicocentral.cloudapp.azure.com"
public_ip             = "158.23.181.69"
resource_group_name   = "rg-eventos-b97e31"
vm_name               = "vm-eventos-b97e31-k3s"
vm_size               = "Standard_B2s_v2"
k3s_version           = "v1.35.2+k3s1"
ssh_command           = "ssh azureuser@eventos-b97e31.mexicocentral.cloudapp.azure.com"
kubernetes_api_tunnel = "ssh -N -L 6443:127.0.0.1:6443 azureuser@eventos-b97e31.mexicocentral.cloudapp.azure.com"
```

## Limitaciones

- Una sola VM y una zona, sin alta disponibilidad (el proyecto no la exige).
- El disco del sistema recibe los tags de la VM solo al crearse: el actual conserva únicamente `project`. Recibirá los demás en el próximo reemplazo de la VM.
- Los applies de esta evidencia se ejecutaron en local con Azure CLI (Owner), con el plan guardado y revisado. El apply por GitHub Actions requiere la configuración del repositorio descrita en DEV-08.
