# Costos de Azure · DEV-14

Límite del proyecto: **US$100 de consumo agregado** en la suscripción Azure for Students asignada. El límite incluye todo lo que consuma la suscripción, no solo este proyecto.

## Estimación

Precios de referencia de pago por uso en `westus`, Linux, en USD. Son **aproximados**: confirmar con la [calculadora de precios de Azure](https://azure.microsoft.com/pricing/calculator/) y con los cargos observados.

| Recurso | Supuesto | Costo estimado |
|---|---|---|
| VM `Standard_B2as_v2` (2 vCPU, 8 GB) | ≈ US$0.075/h encendida | ≈ US$0.90/día con 12 h encendida al día; ≈ US$1.80/día si queda encendida 24 h |
| Disco del sistema StandardSSD 64 GB (E6) | Se cobra aunque la VM esté apagada | ≈ US$0.16/día |
| IP pública Standard estática | Se cobra aunque la VM esté desasignada | ≈ US$0.12/día |
| Storage del state y tráfico | Pocos MB | < US$0.01/día |
| **Total con 12 h/día encendida** | | **≈ US$1.20/día → ≈ US$25 en 3 semanas** |

El margen restante cubre picos de uso (pruebas de carga, defensa) y cargos que no son de este proyecto. Una VM de 4 GB (`Standard_B2als_v2`) reduciría el cómputo a la mitad, pero quedaría al límite de memoria.

## Medidas de control

| Medida | Implementación |
|---|---|
| Apagado diario automático | `azurerm_dev_test_global_vm_shutdown_schedule`: desasigna la VM a las 23:00 (Centroamérica) |
| Encendido solo cuando se usa | `bash infra/scripts/vm.sh start` / `stop`, que desasigna en lugar de solo apagar el SO |
| Presupuesto con alertas | `azurerm_consumption_budget_subscription`: US$100 mensuales; correo al 50 % y 80 % real y al 100 % pronosticado |
| Tamaño acotado | Un solo nodo, disco StandardSSD, sin servicios administrados (PostgreSQL y observabilidad corren en el clúster) |
| Eliminación completa | `terraform destroy` y luego `az group delete --name rg-eventos-tfstate` al cerrar el proyecto |

Las alertas de presupuesto de Azure **notifican pero no detienen recursos**. Una VM desasignada deja de generar cargo de cómputo, pero el disco y la IP pública estática siguen cobrando: para llegar a costo casi nulo hay que destruir los recursos.

## Cargos observados

Registrar los cargos acumulados desde **Cost Management → Cost analysis**, agrupados por Resource Group, al menos antes de cada entrega y antes de la defensa.

| Fecha | Acumulado del mes (suscripción) | `rg-eventos-*` | Observaciones |
|---|---|---|---|
| — | — | — | Pendiente del primer despliegue |
