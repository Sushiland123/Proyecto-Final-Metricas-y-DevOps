# ESC3-01…10 · Evidencia funcional de la parte A

Contrato y comandos: [docs/escenario.md](../../escenario.md).

## Ejecución local · 2026-11-08

Entorno: Linux, ejecutables Rust de debug, HTTP loopback y PostgreSQL 16 en un
contenedor exclusivo de pruebas. **No es una prueba de rendimiento en Azure/K3s.**
No había procesador durante los tres perfiles de carga; se drenó después, sin
nueva entrada. Datos y cuentas ficticios, seed 3.

| Perfil | 202 únicos | 200 duplicados | 422 rechazados | Duración de emisión de únicos | Pérdida de aceptados | Pendientes antes → después |
|---|---:|---:|---:|---:|---:|---:|
| burst | 1000 | 0 | 20 | 1.550 s | 0 | 1000 → 0 |
| extra | 1050 (incluye 50 tardíos) | 50 | 20 | 1.240 s | 0 | 1050 → 0 |
| sustained | 2400 | 0 | 0 | 120.001 s, objetivo 20/s | 0 | 2400 → 0 |

Latencia HTTP p50 / p95 / máxima, en ms:

- burst: 95.65 / 166.68 / 245.87;
- extra: 80.34 / 95.81 / 118.68;
- sustained: 10.82 / 14.91 / 319.87.

Los JSON íntegros se generaron en `artifacts/esc3-{burst,extra,sustained}-local.json`
y el resumen en `artifacts/esc3-local-results.json` (artefactos locales ignorados por
Git, no adjuntos remotos). Incluyen recibos, contadores, errores inciertos y pérdida
calculada comparando los UUID aceptados con el histórico persistido.

**Interpretación de los rechazos:** de los 20 inválidos de burst/extra, 5 usan un
dispositivo inexistente. Por eso el contador filtrado por el dispositivo registrado
muestra 15 rechazados; los otros 5 están en el ledger global/dispositivo inexistente.
El total de HTTP 422 sigue siendo 20. Ningún rechazo se suma a los válidos.

## Reinicio real de componentes

`cargo run --locked --example verify_system` ejecutado con base y puerto dedicados:

```text
PASS: 200 -> 503 -> 200 without restarting API
PASS: event persisted after API + PostgreSQL restart
PASS: ESC3 devices, limits, readings, rejection receipts, alerts and pending work
      survived API + PostgreSQL restart; backlog drained to zero
PASS: failure responses and API logs do not disclose submitted secrets or URL values
```

Se compararon respuestas completas antes/después para configuración del dispositivo,
históricos, agregados, alertas, recibos, rechazos y contadores. Se conservaron una
lectura procesada y una pendiente durante el reinicio, y luego se procesó la pendiente.

## Pruebas ejecutables

Resultado local final: **31 pruebas aprobadas** (20 sin DB y 11 PostgreSQL),
`cargo fmt --all -- --check`, Clippy con `-D warnings` y rustdoc con `-D warnings`
sin errores. Resultados detallados en `artifacts/esc3-unit-local.txt` y
`artifacts/esc3-postgres-local.txt`.

- `tests/scenario_domain.rs`: campos, rangos físicos, límites, formato de IDs y UTC.
- `tests/scenario3/mod.rs` dentro del target `postgres`: 1,000 válidos, 50 duplicados,
  50 tardíos, retransmisiones concurrentes, rechazos y permisos; proceso worker real
  detenido con trabajo pendiente y reanudado sin nueva entrada; agregados y alertas.
- El target PostgreSQL existente ejecuta estas pruebas con `--ignored` en CI;
  no se modificó el workflow de C.

La carga y las comprobaciones locales no acreditan despliegue Kubernetes, aprobación
remota de esta revisión ni los requisitos MET de D. Al publicar el cambio, enlazar
los checks del commit correspondiente; no reutilizar el verde del PR #40 como
evidencia de esta implementación.
