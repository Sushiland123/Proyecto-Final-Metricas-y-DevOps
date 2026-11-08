# Costos de Azure · DEV-14

Límite del proyecto: **US$100 de consumo agregado** en la suscripción Azure for Students asignada. El límite incluye todo lo que consuma la suscripción, no solo este proyecto.

## Estimación

Precios de referencia de pago por uso en `mexicocentral`, Linux, en USD. Son **aproximados**: confirmar con la [calculadora de precios de Azure](https://azure.microsoft.com/pricing/calculator/) y con los cargos observados.

| Recurso | Supuesto | Costo estimado |
|---|---|---|
| VM `Standard_B2s_v2` (2 vCPU, 8 GB) | ≈ US$0.083/h encendida | ≈ US$1.00/día con 12 h encendida al día; ≈ US$2.00/día si queda encendida 24 h |
| Disco del sistema StandardSSD 64 GB (E6) | Se cobra aunque la VM esté apagada | ≈ US$0.16/día |
| IP pública Standard estática | Se cobra aunque la VM esté desasignada | ≈ US$0.12/día |
| Storage del state y tráfico | Pocos MB | < US$0.01/día |
| GitHub Actions (`terraform.yml`, `vm-power.yml`) | Repositorio público | Sin costo |
| **Total con 12 h/día encendida** | | **≈ US$1.30/día → ≈ US$27 en 3 semanas** |
| **Equivalente mensual (30 días, 12 h/día)** | VM ≈ US$30, disco ≈ US$5, IP ≈ US$4 | **≈ US$39/mes** |

El margen restante cubre picos de uso (pruebas de carga, defensa) y cargos que no son de este proyecto. Aun en el peor caso, con la VM encendida 24 h durante 3 semanas, el total estimado (≈ US$48) queda por debajo del límite. Una VM de 4 GB (`Standard_B2als_v2`) reduciría el cómputo a la mitad, pero quedaría al límite de memoria.

## Medidas de control

| Medida | Implementación |
|---|---|
| Apagado diario automático | Workflow [`vm-power.yml`](../../.github/workflows/vm-power.yml): desasigna la VM a las 23:00 (El Salvador, cron `0 5 * * *` UTC) |
| Encendido solo cuando se usa | **Actions → Energía de la VM → Run workflow** (`start` / `stop` / `status`), disponible para todo el equipo sin credenciales de Azure; o `bash infra/scripts/vm.sh start` / `stop` con Azure CLI. Ambos desasignan, no solo apagan el sistema operativo |
| Privilegio mínimo | `id-eventos-ops` solo tiene el rol personalizado `eventos-<sufijo>-vm-operator`: leer, encender y desasignar VMs. No puede crear, modificar ni borrar recursos |
| Presupuesto con alertas | `azurerm_consumption_budget_subscription`: US$100 mensuales; correo al 50 % y 80 % real y al 100 % pronosticado |
| Etiquetas de costo | Los recursos de Terraform llevan `project`, `environment`, `cost_center` y `managed_by`. En Cost Management se filtra por `cost_center = proyecto-catedra-2026` o por el Resource Group. El disco del sistema recibe las etiquetas de la VM solo al crearse: el actual conserva únicamente `project`, así que para él se usa el filtro por Resource Group |
| Tamaño acotado | Un solo nodo, disco StandardSSD, sin servicios administrados (PostgreSQL y observabilidad corren en el clúster) |
| Eliminación completa | `terraform destroy` y luego `az group delete --name rg-eventos-tfstate` al cerrar el proyecto |

**Por qué no el apagado automático de Azure.** `Microsoft.DevTestLab/schedules` no está disponible en `mexicocentral` (`LocationNotAvailableForResourceType`). Azure exige que el schedule esté en la región de la VM (`InvalidScheduleLocation`), así que tampoco se puede crear en otra región. Mover la VM a una región con DevTestLab contradiría la configuración validada por el docente.

**Comportamiento comprobado.** Al desasignar y volver a encender la VM, la IP pública estática y el DNS se conservan, y K3s vuelve a `Ready` sin intervención en unos 90 segundos. Los Pods del sistema se reinician una vez; los volúmenes `local-path` están en el disco del sistema y se conservan.

Las alertas de presupuesto de Azure **notifican pero no detienen recursos**. Una VM desasignada deja de generar cargo de cómputo, pero el disco y la IP pública estática siguen cobrando: para llegar a costo casi nulo hay que destruir los recursos.

## Cargos observados

Registrar los cargos acumulados desde **Cost Management → Cost analysis**, agrupados por Resource Group, al menos antes de cada entrega y antes de la defensa. Los cargos tardan hasta 24 horas en aparecer.

| Fecha | Acumulado del mes (suscripción) | `rg-eventos-*` | Observaciones |
|---|---|---|---|
| — | — | — | Primer despliegue en `mexicocentral` el 2026-11-08; registrar el 2026-11-09 |
