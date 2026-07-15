FROM rust:1.88-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bin liquidlane-core

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/liquidlane-core /usr/local/bin/liquidlane-core
COPY railway/core/entrypoint.sh /usr/local/bin/liquidlane-core-entrypoint
RUN chmod +x /usr/local/bin/liquidlane-core-entrypoint
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/liquidlane-core-entrypoint"]
