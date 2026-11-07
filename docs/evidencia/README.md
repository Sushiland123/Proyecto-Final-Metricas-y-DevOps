# Evidencia para la defensa

Cada requisito (`COM-01` … `COM-06`, `DEV-01` … `DEV-14`) debe tener evidencia verificable enlazada desde su issue.

## Migración Rust

Los índices COM enlazan código y pruebas Rust. El historial de la versión anterior permanece en Git (`6ef42ba`); sus resultados no acreditan la nueva implementación. CI produce `rust-api-and-code-docs` y `rust-postgres-evidence`. Enlazar la ejecución real del PR cuando exista, sin inventar URLs ni dar por ejecutado GitHub Actions desde una prueba local. El experimento real está en `examples/verify_system.rs`; conserva su volumen y deja logs en `artifacts/`.

## Estructura

```text
docs/evidencia/
  COM-01/
    index.md
    2026-10-20_reporte-aceptacion.png
  COM-02/
    arquitectura.mmd
    decisiones.md
  DEV-14/
    estimacion-costos.md
    cargos-observados.png
```

- Una carpeta por ID de requisito.
- Un `index.md` por carpeta que explique qué demuestra cada archivo y cómo reproducirlo.
- Nombres con fecha (`YYYY-MM-DD_descripcion.ext`) para conservar el historial.
- Los artefactos pesados (planes de Terraform, SBOM, reportes JUnit/HTML) se publican como artefactos del workflow y se enlazan; no se commitean.

## Contenido mínimo de un `index.md`

1. **Requisito y criterio de aceptación** cubierto.
2. **Comando o pasos** para reproducir la evidencia.
3. **Resultado esperado** y **resultado obtenido**.
4. **Enlaces** al PR, workflow run, issue y archivos adjuntos.
5. **Limitaciones o riesgos** conocidos.

## Reglas

- No incluir secretos, tokens, `kubeconfig`, estado de Terraform ni IDs de suscripción sin redactar.
- La evidencia debe corresponder a una ejecución real, no a una captura de diseño.
- Si un requisito no aplica (por ejemplo, COM-05), documentar la justificación con la misma rigurosidad.
- La evidencia se enlaza desde el issue antes de cerrarlo.
