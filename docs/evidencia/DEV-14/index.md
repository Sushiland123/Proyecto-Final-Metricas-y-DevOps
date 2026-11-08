# DEV-14 · Control del consumo de Azure (límite US$100)

[Issue #20](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/20) · [PR #40](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/pull/40). Criterios:

- estimación alineada con Terraform;
- alertas de presupuesto configuradas;
- procedimiento de detención y eliminación probado;
- cargos observados documentados, sin superar US$100.

Estimación, medidas y registro de cargos: [docs/infra/costos.md](../../infra/costos.md).

## Resultado obtenido · 2026-11-08

**Estimación** (alineada con los recursos de Terraform):

- ≈ US$1.30/día con la VM encendida 12 h;
- ≈ US$39/mes;
- ≈ US$27 en 3 semanas;
- peor caso, 24 h durante 3 semanas: ≈ US$48.

**Presupuesto** (`terraform state show 'azurerm_consumption_budget_subscription.project[0]'`):

```text
name       = "budget-eventos-b97e31"
amount     = 100
time_grain = "Monthly"
start_date = "2026-11-01T00:00:00Z"
threshold 50  Actual
threshold 80  Actual
threshold 100 Forecasted
```

Los correos de alerta son los de `budget_contact_emails`.

**Detención probada.** `bash infra/scripts/vm.sh stop` desasignó la VM (`VM deallocated`) en 34 s y `start` la encendió en 44 s. K3s volvió a `Ready` sin intervención: ver DEV-13.

**Apagado diario.** El apagado nativo de Azure (`Microsoft.DevTestLab/schedules`) no está disponible en `mexicocentral`. Se probaron las dos variantes y Azure las rechazó:

```text
location = mexicocentral → LocationNotAvailableForResourceType
location = westus3       → InvalidScheduleLocation: The schedule should be created in location mexicocentral.
```

Se reemplazó por [`vm-power.yml`](../../../.github/workflows/vm-power.yml), que desasigna la VM a las 23:00 y permite `start`, `stop` y `status` manuales desde GitHub. Usa `id-eventos-ops`, con un rol personalizado limitado a cuatro acciones (`virtualMachines/read`, `instanceView/read`, `start/action`, `deallocate/action`). Los nombres de las acciones se validaron contra `az provider operation show`.

**Eliminación probada.** El cambio de región destruyó por Terraform todos los recursos regionales anteriores, sin recursos huérfanos: el grupo antiguo se eliminó completo.

## Pendiente

- [ ] Ejecutar el bootstrap actualizado, que crea `id-eventos-ops` y su rol, y configurar el environment `azure-ops`.
- [ ] Enlazar la primera ejecución programada de `vm-power.yml` (`VM deallocated`).
- [ ] Registrar cada semana, y antes de cada entrega y de la defensa, el acumulado del mes, el acumulado del proyecto, el crédito restante y el saldo hasta US$100 (tabla de `costos.md`). El presupuesto mensual no cubre el agregado de varios meses.

## Limitaciones

- Las alertas de presupuesto notifican, pero no detienen recursos.
- Con la VM desasignada siguen cobrándose el disco y la IP estática (≈ US$0.28/día).
- Hasta configurar `vm-power.yml`, el apagado es manual (`vm.sh stop`).
