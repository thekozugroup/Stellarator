# syntax=docker/dockerfile:1
# Multi-arch (linux/amd64, linux/arm64): every base image is multi-arch and the Rust build is
# native to the build platform, so the same file builds on an Apple Silicon Mac, a DGX Spark
# (arm64) or an x86 server.
#
# The web UI is the nsh app's web build (app/, `nsh package --target web`). The React UI
# (frontend/) is still there until the switch is done: `docker build --build-arg WEB_UI=react .`
#
# Optional build secret "ca": a PEM CA bundle for networks that intercept TLS:
#   docker build --secret id=ca,src=/path/ca.pem .

ARG WEB_UI=nsh

# ---- 1. The web UI ------------------------------------------------------------------------
# The nsh web build is WebAssembly and the same for every platform, so it runs on the build
# machine's own platform. `nsh package` also compiles the app's crate natively (to describe its
# UI), which links GTK 4 (4.14 or later: Debian 13).
FROM --platform=$BUILDPLATFORM rust:1-trixie AS web-nsh
RUN apt-get update \
 && apt-get install -y --no-install-recommends libgtk-4-dev clang llvm curl ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && rustup target add wasm32-unknown-unknown
RUN --mount=type=secret,id=ca,required=false \
    if [ -f /run/secrets/ca ]; then export SSL_CERT_FILE=/run/secrets/ca CURL_CA_BUNDLE=/run/secrets/ca; fi; \
    curl -fsSL https://nsh.dev/install.sh | sh \
 && /root/.local/bin/nsh --version
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY app ./app
# The web runtime component downloads on first use, as in CI.
RUN --mount=type=secret,id=ca,required=false \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/app/target \
    if [ -f /run/secrets/ca ]; then export SSL_CERT_FILE=/run/secrets/ca CARGO_HTTP_CAINFO=/run/secrets/ca; fi; \
    cd app && mkdir -p .nsh/rust && cp Cargo.lock .nsh/rust/Cargo.lock \
 && CARGO_TARGET_DIR=/src/app/target /root/.local/bin/nsh package --target web --out /out \
 && mv /out/Stellarator /web

# The React UI (WEB_UI=react).
FROM node:22-slim AS web-react
WORKDIR /web-src
COPY frontend/package.json frontend/package-lock.json ./
RUN --mount=type=secret,id=ca,required=false \
    if [ -f /run/secrets/ca ]; then export NODE_EXTRA_CA_CERTS=/run/secrets/ca; fi; \
    npm ci --no-audit --no-fund
COPY frontend/ ./
RUN npm run build && mv dist /web

FROM web-${WEB_UI} AS web

# ---- 2. Build the server (the web UI is embedded in the binary) --------------------------
FROM rust:1-bookworm AS server
ARG WEB_UI
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# Where crates/server/build.rs looks for each UI.
COPY --from=web /web ./app/.nsh/web/Stellarator
COPY --from=web /web ./frontend/dist
RUN --mount=type=secret,id=ca,required=false \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    if [ -f /run/secrets/ca ]; then export SSL_CERT_FILE=/run/secrets/ca CARGO_HTTP_CAINFO=/run/secrets/ca; fi; \
    STELLARATOR_WEB_UI=$WEB_UI cargo build --release --locked -p stellarator-server -p stellarator-mock \
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
