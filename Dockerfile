FROM rust:1.91-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked --bins

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/liquidlane-core /usr/local/bin/liquidlane-core
COPY --from=builder /app/target/release/liquidlane-connector /usr/local/bin/liquidlane-connector
COPY railway/core/entrypoint.sh /usr/local/bin/liquidlane-core-entrypoint
RUN chmod +x /usr/local/bin/liquidlane-core-entrypoint
RUN useradd --system --uid 10001 liquidlane && mkdir /data && chown liquidlane /data
USER liquidlane
ENV LIQUIDLANE_PRODUCT_MODE=marketplace
ENV LIQUIDLANE_MARKET_DB=/data/liquidlane-marketplace.sqlite3
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s CMD curl -fsS "http://127.0.0.1:${PORT:-8080}/health" || exit 1
ENTRYPOINT ["/usr/local/bin/liquidlane-core-entrypoint"]
