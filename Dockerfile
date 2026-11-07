FROM rust:1.97.1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY src ./src
COPY migrations ./migrations
COPY docs/api.html ./docs/api.html
ARG CARGO_BUILD_JOBS=2
RUN cargo build --release --locked --jobs ${CARGO_BUILD_JOBS}

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 app && useradd --uid 10001 --gid app --no-create-home app
COPY --from=builder /app/target/release/event-service /usr/local/bin/event-service
USER 10001:10001
ENV BIND_ADDR=0.0.0.0:8000
EXPOSE 8000
ENTRYPOINT ["event-service"]
CMD ["serve"]
