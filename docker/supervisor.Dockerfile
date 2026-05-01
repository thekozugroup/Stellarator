# syntax=docker/dockerfile:1.6
# Multi-stage build for the Stellarator Rust supervisor sidecar.

FROM rust:1-bookworm AS builder
WORKDIR /build

# Cache deps separately from sources.
COPY supervisor/Cargo.toml supervisor/Cargo.toml
RUN mkdir -p supervisor/src supervisor/tests \
 && echo "fn main() {}" > supervisor/src/main.rs \
 && echo "" > supervisor/src/lib.rs
WORKDIR /build/supervisor
RUN cargo fetch

# Real build.
COPY supervisor/ /build/supervisor/
RUN cargo build --release --locked || cargo build --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates libssl3 sqlite3 \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /build/supervisor/target/release/stellarator-supervisor /usr/local/bin/stellarator-supervisor

ENV RUST_LOG=info \
    SUPERVISOR_BIND=0.0.0.0:8001 \
    STELLARATOR_DB_URL=sqlite:///data/stellarator.db

EXPOSE 8001
CMD ["stellarator-supervisor"]
