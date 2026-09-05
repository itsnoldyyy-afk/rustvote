FROM rust:latest AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
COPY templates ./templates
COPY static ./static

RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
ARG RUSTVOTE_DEPLOY_VERSION=2
RUN mkdir -p /app/data && chmod 777 /app/data

COPY --from=builder /app/target/release/rustvote /app/rustvote
COPY --from=builder /app/migrations /app/migrations
COPY --from=builder /app/templates /app/templates
COPY --from=builder /app/static /app/static

ENV HOST=0.0.0.0
ENV DATABASE_URL=sqlite::memory:

EXPOSE 10000
CMD ["/app/rustvote"]
