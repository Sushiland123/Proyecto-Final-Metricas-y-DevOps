# Las bases se fijan por digest: un tag puede moverse y romper la reconstrucción
# desde un commit. Para actualizarlas se cambia el digest en un PR.
FROM rust:1.97.1-bookworm@sha256:0e2bcaef56d041a486784e54104a81aebe0da44bd03019bd70bc0401e42e4a97 AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY src ./src
COPY migrations ./migrations
COPY docs/api.html ./docs/api.html
ARG CARGO_BUILD_JOBS=2
RUN cargo build --release --locked --jobs ${CARGO_BUILD_JOBS}

# Distroless: solo glibc, libgcc y certificados CA. No hay shell ni gestor de paquetes.
# El diagnóstico usa logs, eventos y `kubectl debug`, no `kubectl exec`.
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f AS runtime
ARG VERSION=dev
ARG REVISION=unknown
ARG SOURCE=https://github.com/marchelo23/Proyecto-Final-Metricas-y-DevOps
LABEL org.opencontainers.image.title="event-service" \
      org.opencontainers.image.description="API y procesador de eventos del Escenario 3" \
      org.opencontainers.image.source="${SOURCE}" \
      org.opencontainers.image.revision="${REVISION}" \
      org.opencontainers.image.version="${VERSION}"
COPY --from=builder /app/target/release/event-service /usr/local/bin/event-service
# UID numérico: Kubernetes solo puede comprobar runAsNonRoot con un número.
USER 10001:10001
ENV BIND_ADDR=0.0.0.0:8000
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/event-service"]
CMD ["serve"]
