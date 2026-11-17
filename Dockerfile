# syntax=docker/dockerfile:1
# Multi-arch (linux/amd64, linux/arm64): every base image is multi-arch and the Rust build is
# native to the build platform, so the same file builds on an Apple Silicon Mac, a DGX Spark
# (arm64) or an x86 server.

# ---- 1. Build the web UI ----------------------------------------------------------------
FROM node:22-slim AS web
WORKDIR /web
COPY frontend/package.json frontend/package-lock.json ./
# Optional build secret "ca": a PEM CA bundle for networks that intercept TLS:
#   docker build --secret id=ca,src=/path/ca.pem .
RUN --mount=type=secret,id=ca,required=false \
    if [ -f /run/secrets/ca ]; then export NODE_EXTRA_CA_CERTS=/run/secrets/ca; fi; \
    npm ci --no-audit --no-fund
COPY frontend/ ./
RUN npm run build

# ---- 2. Build the server (the web UI is embedded in the binary) --------------------------
FROM rust:1-bookworm AS server
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=web /web/dist ./frontend/dist
RUN --mount=type=secret,id=ca,required=false \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    if [ -f /run/secrets/ca ]; then export SSL_CERT_FILE=/run/secrets/ca CARGO_HTTP_CAINFO=/run/secrets/ca; fi; \
    cargo build --release --locked -p stellarator-server -p stellarator-mock \
 && mkdir -p /out && cp target/release/stellarator-server target/release/stellarator-mock /out/

# ---- 3. Runtime -------------------------------------------------------------------------
FROM debian:bookworm-slim
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd --system --uid 10001 --create-home --home-dir /home/stellarator stellarator \
 && mkdir /data && chown stellarator:stellarator /data
COPY --from=server /out/stellarator-server /out/stellarator-mock /usr/local/bin/
ENV STELLARATOR_DATA_DIR=/data \
    STELLARATOR_BIND=0.0.0.0:8080 \
    STELLARATOR_DOCKER=1 \
    RUST_LOG=info
USER stellarator
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["stellarator-server", "healthcheck"]
# One process: the run engine and import jobs keep their state in it.
CMD ["stellarator-server"]
