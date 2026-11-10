# DEV-05 · Entrega controlada

[Issue #11](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/11).

## Flujo

[`deliver.yml`](../../../.github/workflows/deliver.yml) se lanza a mano desde `main` con un commit ya publicado:

1. **Resolver:** comprueba que el commit pertenece a `main`, toma el digest de su publicación y verifica la firma. La firma debe corresponder a ese commit y a `publish-image.yml`.
2. **development:** migración, rollout, comprobación del digest en los Pods y smoke.
3. **Aprobación:** el environment `production` espera a un revisor.
4. **production:** el mismo digest y los mismos pasos. No se reconstruye nada.

Si falla la migración, los workloads no cambian. Si falla el rollout o el smoke, [`k8s-deliver.sh`](../../../scripts/k8s-deliver.sh) restaura cada Deployment que la entrega cambió a la revisión capturada antes de empezar, comprueba su digest y repite el smoke. Un Deployment que no cambió no se toca. El job falla, así que production no se ejecuta.

## Decisiones

- **Azure Run Command.** El puerto 6443 sigue cerrado y SSH solo acepta IP administradas, así que los runners no llegan al clúster. Run Command no abre puertos ni guarda credenciales.
- **Riesgo aceptado.** Run Command ejecuta como root en la VM, así que la identidad de despliegue tiene control total de la VM. Se limita con OIDC restringido a los environments, un rol acotado a la VM y workflows solo desde `main`. Las escrituras en el clúster usan `event-service-deployer`.
- **Manual y no en cada push.** La VM pasa apagada para controlar costos, y un disparador automático fallaría casi siempre.
- **Actions fijadas por commit** en los jobs con `id-token: write`.
- **El bundle no lleva secretos.** Solo manifiestos y scripts; las claves se leen en el clúster y nunca se imprimen.

## Exposición pública

Solo production se publica, con [`k8s-ingress.sh`](../../../scripts/k8s-ingress.sh). El nombre DNS es la etiqueta de la IP de Azure y el certificado lo emite Let's Encrypt mediante cert-manager, en versión y checksum fijos. La API se sirve solo por HTTPS y exige `X-API-Key`. Development no tiene Ingress.

## Qué versión corre en cada ambiente

```bash
sudo k3s kubectl -n eventos-production get deployment event-service-api \
  -o jsonpath='{.spec.template.spec.containers[0].image}'
gh attestation verify oci://<imagen>@<digest> --repo marchelo23/Proyecto-Final-Metricas-y-DevOps
```

## Puesta en marcha

Se hace una vez. `k8s-bootstrap.sh` e `k8s-ingress.sh` son intervenciones manuales declaradas, porque crean recursos que la cuenta de despliegue no puede crear.

```bash
KUBECTL="sudo k3s kubectl" bash scripts/k8s-bootstrap.sh development
KUBECTL="sudo k3s kubectl" bash scripts/k8s-bootstrap.sh production
KUBECTL="sudo k3s kubectl" bash scripts/k8s-ingress.sh <fqdn>
```

## Resultado obtenido · 2026-11-09, ensayo local

**No es la evidencia final.** K3s `v1.35.2+k3s1` desechable en Docker, con un registro local como espejo de GHCR. El workflow no se ejecutó; el script que envía a la VM sí.

| Prueba | Resultado |
|---|---|
| Entrega a development | `ENTREGA_OK` en 15 s, digest comprobado en los Pods y lectura procesada |
| Repetir el mismo digest | `ENTREGA_OK`, sin cambios |
| Entrega defectuosa, imagen sin `worker-health` | El rollout falla, vuelve a la versión anterior, código de salida 1, 191 s |
| Bootstrap y entrega a production | `ENTREGA_OK` en 17 s |
| HTTPS por Ingress | `/health/live` 200 y sin clave 401 |
| Mismo Ingress por HTTP | 404: la API no se sirve sin cifrar |
| Reto ACME con NetworkPolicy | Alcanzable, 200 |
| `gh attestation verify` sobre la imagen publicada | Correcta, con el commit de la firma |
| `actionlint` y `shellcheck` | Sin hallazgos |

## Pendiente

- [ ] Identidad de despliegue en Azure, environments y `DEPLOY_CLIENT_ID`.
- [ ] Primera entrega real, con aprobación y certificado emitido.
- [ ] Demostrar en Actions que un smoke fallido impide completar la entrega.
