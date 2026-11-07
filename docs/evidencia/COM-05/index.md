# COM-05 · Acceso y segregación · Rust

[Issue #5](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/5). Criterio: roles y fuentes restringidas, rechazo sin credenciales y sin escalamiento.

```bash
cargo test --locked --test auth --test http --test service
cargo test --locked --test postgres -- --ignored
```

| Condición | Comprobación |
|---|---|
| producer no puede leer / analyst no puede escribir | `roles_and_source_scope_cannot_escalate`, `service_rechecks_permissions_before_repository_access` |
| Clave por fuente no puede escribir otra fuente | `health_and_uniform_safe_errors`, `batch_is_atomic_and_source_scope_is_enforced` |
| Configuración parcial, fuentes duplicadas o colisiones | `malformed_or_colliding_settings_fail_closed` |
| Una clave restringida para varias fuentes | `one_scoped_key_may_authorize_multiple_sources` |
| Cabeceras API-key duplicadas | `request_id_is_preserved_or_replaced_and_duplicate_keys_rejected` |

Resultados esperados: 401 sin autenticación/configuración válida, 403 por rol/fuente, ninguna llamada al repositorio en casos de uso denegados. Detalles en [acceso](../../arquitectura/acceso.md). Configuración de inicio, sin revocación dinámica ni IdP; TLS y rate limiting deben configurarse fuera del servicio.
