# DEV-04 · Automatizar la construcción y publicación de imágenes

[Issue #10](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/10). Criterios: cada push a `main` produce una imagen versionada y trazable, se puede ir del commit al digest desplegado, las credenciales son de mínimo privilegio y un PR no confiable no puede publicar.

## Flujo

[`publish-image.yml`](../../../.github/workflows/publish-image.yml) corre con cada push a `main` y con cada tag `vX.Y.Z`:

1. Construye una sola vez y publica en `ghcr.io/marchelo23/proyecto-final-metricas-y-devops/event-service`, con SBOM y procedencia de BuildKit.
2. Descarga la imagen por digest y comprueba que su label `revision` sea el commit. Luego ejecuta [`verify-image.sh`](../../../scripts/verify-image.sh).
3. Firma la procedencia con `attest-build-provenance` y la comprueba con `gh attestation verify`.
4. Publica el artefacto `image-publication-<commit>`, con `publicacion.json`, reportes y SBOM. También escribe el resumen de la ejecución.

## Decisiones

- **Sin `latest`.** Un tag móvil no identifica una versión. Los tags son `sha-<corto>` y `X.Y.Z`, y lo que se despliega es el digest.
- **Solo `GITHUB_TOKEN`.** Caduca al terminar el job y no hay credenciales guardadas. `packages: write` solo existe en este job.
- **Sin disparador `pull_request`.** Un PR, incluso desde un fork, no puede publicar imágenes.
- **Verificar después de publicar.** Así se comprueban los bytes exactos que están en el registro. Si la verificación falla, la imagen queda en el registro sin attestation ni `publicacion.json`, y la entrega no debe promoverla.

## Cómo seguir la trazabilidad

```bash
# Del digest al commit y al workflow que lo construyó:
gh attestation verify oci://<imagen>@<digest> --repo marchelo23/Proyecto-Final-Metricas-y-DevOps
# Del commit al digest: artefacto image-publication-<commit> en la ejecución de "Publicar imagen".
```

## Resultado obtenido · 2026-11-08, prueba local

Se probó con un registro local temporal la parte que no depende de GHCR: publicar, descargar por digest y ejecutar `verify-image.sh` sobre esa referencia. El resultado fue correcto. `actionlint` no reportó errores en el workflow.

## Resultado en `main`

- Publicación: commit `ad1c0c6`, [ejecución de publicación](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/actions/runs/42542020614), digest `sha256:bd50b3cf03f23eb754c3635110222330ac6cba76b404101020eb091ef2845095`.
- Tag `sha-ad1c0c6`, sin `latest`. El artefacto `image-publication-<commit>` guarda el vínculo commit → ejecución → digest.
- Firma comprobada con `gh attestation verify`; el `gitCommit` firmado es `ad1c0c65a1e4d225f9e028eb35d665159f286135`.
- El paquete de GHCR es público: el clúster lo descarga sin credenciales.
- La imagen desplegada corresponde al commit: el `imageID` de los Pods en Azure es ese digest ([DEV-09](../DEV-09/index.md)).

## Limitaciones

La firma prueba qué workflow y qué commit produjeron la imagen. No prueba que ese código sea correcto ni que la imagen esté libre de vulnerabilidades.
