# Proyecto Final — Métricas y DevOps

Repositorio de trabajo del proyecto final. El backlog está compuesto por **27 issues** que cubren los requisitos evaluables de los grupos **3.1 Aplicación y arquitectura**, **3.2 DevOps** y **3.3 Métricas de Software y observabilidad**, cada uno con su evidencia mínima, trabajo propuesto, criterios de aceptación y Definition of Done.

> **Nombre del repositorio:** GitHub no admite espacios en el nombre, por lo que normalizó el solicitado *"Proyecto Final Metricas y DevOps"* a `Proyecto-Final-Metricas-y-DevOps`. Esta es la URL canónica.

- **URL:** <https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps>
- **Milestones:** `3.1 Aplicación y arquitectura`, `3.2 DevOps` y `3.3 Métricas de Software y observabilidad`
- **Escenario asignado:** **Escenario 3: Ingesta y Análisis de Eventos** (requisitos COM-01 a COM-06).

---

## Escenario 3: Ingesta y Análisis de Eventos (COM-01 a COM-06)

Se implementa una plataforma de ingesta y análisis analítico de eventos sobre FastAPI y PostgreSQL 16 con SQLAlchemy 2 y psycopg 3:
- **Ingesta individual (`POST /v1/events`) y en lote atómico (`POST /v1/events/batch`):** Inserción transaccional con deduplicación estricta por tupla `(source, external_id)`.
- **Exploración de eventos (`GET /v1/events` y `GET /v1/events/{id}`):** Consulta puntual por UUID y listado paginado (`limit`, `offset`) con filtros opcionales por fuente y tipo sobre ventana semiabierta UTC `[from, to)`.
- **Análisis y agregaciones (`GET /v1/analysis/*`):** Conteos dimensionales por tipo o fuente (`/counts`), análisis de series temporales por intervalos (`/timeseries` con buckets `minute`, `hour`, `day`) y resúmenes estadísticos (`/summary` con min, max, avg de campos numéricos del JSONB).
- **Control de acceso y segregación (COM-05):** RBAC estricto mediante cabecera `X-API-Key` (`producer` para ingesta, `analyst` para lectura) y soporte de aislamiento lateral por fuente emisora (`EVENT_PRODUCER_SOURCE_KEYS`).
- **Resiliencia y manejo de errores (COM-04):** Manejo de errores seguros y estructurados con `X-Request-ID`, `pool_pre_ping=True`, timeouts acotados y reintentos con backoff en lecturas operacionales.

Consulte la [matriz de reglas y contratos del Escenario 3](docs/escenario.md) para la trazabilidad completa regla → prueba.

---

## Arranque local reproducible

Requiere Python 3.12 o 3.13, [`uv`](https://docs.astral.sh/uv/) y Docker Compose o Podman Compose. Desde la raíz del repositorio:

```bash
uv sync --locked

cat > .env <<EOF
POSTGRES_USER=events
POSTGRES_DB=events_test
POSTGRES_PORT=15433
POSTGRES_PASSWORD=$(openssl rand -hex 16)
EVENT_PRODUCER_KEY=$(openssl rand -hex 16)
EVENT_ANALYST_KEY=$(openssl rand -hex 16)
EVENT_PRODUCER_SOURCE_KEYS=sensor-a:$(openssl rand -hex 16),sensor-b:$(openssl rand -hex 16)
EOF
set -a; . ./.env; set +a
cat >> .env <<EOF
DATABASE_URL=postgresql+psycopg://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}
TEST_DATABASE_URL=postgresql+psycopg://${POSTGRES_USER}:${POSTGRES_PASSWORD}@127.0.0.1:${POSTGRES_PORT}/${POSTGRES_DB}
EOF
set -a; . ./.env; set +a
chmod 600 .env

# Iniciar contenedor PostgreSQL de prueba
docker compose up -d postgres

# Esperar a que PostgreSQL esté listo
for intento in $(seq 1 30); do
  if docker compose exec -T postgres pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB"; then
    break
  fi
  sleep 1
done

# Aplicar migraciones versionadas (incluye tabla e índices compuestos)
uv run --locked alembic upgrade head

# Iniciar servicio web
uv run uvicorn event_service.main:app --host 127.0.0.1 --port 8000
```

`.env` está en `.gitignore`: no lo commitees. En otra terminal con el mismo `.env`, prueba la API:

```bash
set -a; . ./.env; set +a

# 1. Sondas de salud
curl -i http://127.0.0.1:8000/health/live
curl -i http://127.0.0.1:8000/health/ready

# 2. Ingesta puntual (Rol producer)
curl -i -H "X-API-Key: $EVENT_PRODUCER_KEY" -H 'Content-Type: application/json' \
  -d '{"source":"sensor-a","external_id":"e-001","type":"temperature","occurred_at":"2026-10-21T12:00:00Z","payload":{"value":23.5}}' \
  http://127.0.0.1:8000/v1/events

# 3. Ingesta en lote atómica (Rol producer)
curl -i -H "X-API-Key: $EVENT_PRODUCER_KEY" -H 'Content-Type: application/json' \
  -d '{"events":[{"source":"sensor-a","external_id":"e-002","type":"temperature","occurred_at":"2026-10-21T12:05:00Z","payload":{"value":24.1}},{"source":"sensor-a","external_id":"e-003","type":"temperature","occurred_at":"2026-10-21T12:10:00Z","payload":{"value":22.8}}]}' \
  http://127.0.0.1:8000/v1/events/batch

# 4. Exploración paginada (Rol analyst)
curl -G -H "X-API-Key: $EVENT_ANALYST_KEY" \
  --data-urlencode 'from=2026-10-21T00:00:00Z' --data-urlencode 'to=2026-10-22T00:00:00Z' \
  --data-urlencode 'source=sensor-a' --data-urlencode 'limit=10' \
  http://127.0.0.1:8000/v1/events

# 5. Agregación y conteos por dimensión (Rol analyst)
curl -G -H "X-API-Key: $EVENT_ANALYST_KEY" \
  --data-urlencode 'from=2026-10-21T00:00:00Z' --data-urlencode 'to=2026-10-22T00:00:00Z' \
  --data-urlencode 'group_by=type' http://127.0.0.1:8000/v1/analysis/counts

# 6. Análisis de series temporales (Rol analyst)
curl -G -H "X-API-Key: $EVENT_ANALYST_KEY" \
  --data-urlencode 'from=2026-10-21T00:00:00Z' --data-urlencode 'to=2026-10-22T00:00:00Z' \
  --data-urlencode 'bucket=hour' http://127.0.0.1:8000/v1/analysis/timeseries

# 7. Resumen estadístico sobre carga útil (Rol analyst)
curl -G -H "X-API-Key: $EVENT_ANALYST_KEY" \
  --data-urlencode 'from=2026-10-21T00:00:00Z' --data-urlencode 'to=2026-10-22T00:00:00Z' \
  --data-urlencode 'numeric_field=value' http://127.0.0.1:8000/v1/analysis/summary

# 8. Ejecución de la suite completa de pruebas
mkdir -p artifacts
uv run --locked pytest --junitxml=artifacts/junit.xml --html=artifacts/acceptance.html --self-contained-html
```

---

## Backlog

### 3.1 Aplicación y arquitectura (Escenario 3)

| ID | Requisito | Evidencia mínima | Issue |
|---|---|---|---|
| COM-01 | Implementar las funciones y reglas del escenario | Pruebas de aceptación ejecutables y resultados verificables | [#1](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/1) |
| COM-02 | Definir y justificar límites y responsabilidades | Diagrama de arquitectura real y decisiones | [#2](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/2) |
| COM-03 | Persistir la información exigida | Persistencia ante reinicio de componentes | [#3](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/3) |
| COM-04 | Manejar errores y validar entradas | Pruebas positivas, negativas y de recuperación | [#4](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/4) |
| COM-05 | Controles de acceso cuando corresponda | Pruebas de acceso autorizado y rechazado | [#5](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/5) |
| COM-06 | Trade-offs de consistencia, disponibilidad, costo y complejidad | Registro de decisiones y alternativas descartadas | [#6](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/6) |

### 3.2 DevOps

| ID | Requisito | Evidencia mínima | Issue |
|---|---|---|---|
| DEV-01 | Versionar aplicación, infraestructura y configuración no secreta | Repositorio e historial con PRs revisados | [#7](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/7) |
| DEV-02 | Distribuir componentes como imágenes de contenedor | Dockerfiles, imágenes versionadas y trazables | [#8](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/8) |
| DEV-03 | Integración continua con GitHub Actions | Checks en PR y protección de rama | [#9](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/9) |
| DEV-04 | Automatizar build y publicación de imágenes | Vínculo commit → workflow → imagen | [#10](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/10) |
| DEV-05 | Entrega controlada con validación y producción separadas | Workflow, aprobaciones y despliegue | [#11](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/11) |
| DEV-06 | Recursos principales de Azure con Terraform | Código, plan revisable y apply controlado | [#12](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/12) |
| DEV-07 | Estado remoto de Terraform protegido | Backend y procedimiento de inicialización | [#13](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/13) |
| DEV-08 | Validación de Terraform en GitHub Actions | Validación y plan sin apply en PRs no aprobados | [#14](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/14) |
| DEV-09 | Instalar y operar K3s en VM de Azure | Nodos y workloads verificables | [#15](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/15) |
| DEV-10 | Separar configuración y secretos; privilegio mínimo | Configuración, secretos y evidencia de permisos | [#16](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/16) |
| DEV-11 | Salud y recursos de los workloads | Probes, requests y límites justificados | [#17](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/17) |
| DEV-12 | Recuperar una versión funcional tras un despliegue defectuoso | Demostración de rollback y verificación | [#18](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/18) |
| DEV-13 | Reconstruir el ambiente con procedimiento documentado | Instrucciones y prerrequisitos explícitos | [#19](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/19) |
| DEV-14 | Consumo de Azure bajo US$100 | Estimación, cargos, alertas y detención/eliminación | [#20](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/20) |

### 3.3 Métricas de Software y observabilidad

| ID | Requisito | Evidencia mínima | Issue |
|---|---|---|---|
| MET-01 | Definir indicadores técnicos y de comportamiento | Catálogo con definición, unidad, fuente, cálculo y propósito | [#21](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/21) |
| MET-02 | Incorporar logs estructurados | Registros con identificadores de correlación | [#22](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/22) |
| MET-03 | Instrumentar métricas de aplicación e infraestructura | Consultas y series temporales verificables | [#23](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/23) |
| MET-04 | Instrumentar trazas con OpenTelemetry | Trazas consultables y correlación entre señales | [#24](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/24) |
| MET-05 | Construir dashboards para preguntas operativas | Paneles interpretables con unidades y ventanas | [#25](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/25) |
| MET-06 | Configurar alertas justificadas | Regla, activación y análisis de falsos positivos | [#26](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/26) |
| MET-07 | Analizar una degradación inducida y su recuperación | Secuencia temporal, evidencia, interpretación y límites | [#27](https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps/issues/27) |

---

## Evidencia y Arquitectura

- [Especificación formal y matriz regla → test del Escenario 3](docs/escenario.md)
- [Vista de contexto y límites (C4 Context)](docs/arquitectura/contexto.md)
- [Vista de contenedores y módulos (C4 Container)](docs/arquitectura/contenedores.md)
- [Vista de despliegue](docs/arquitectura/despliegue.md)
- [Persistencia y durabilidad de datos](docs/arquitectura/persistencia.md)
- [Manejo de errores, validación y recuperación](docs/arquitectura/errores.md)
- [Control de acceso y segregación](docs/arquitectura/acceso.md)
- [Registro de compromisos y trade-offs (COM-06)](docs/arquitectura/tradeoffs.md)
- [ADRs del proyecto](docs/adr/0001-descomposicion.md)
- Carpeta de evidencias por requisito: `docs/evidencia/COM-0X/`
