# syntax=docker/dockerfile:1
# Multi-arch (linux/amd64, linux/arm64). Every base image below is a multi-arch image and
# every Python dependency installs from a manylinux/musllinux wheel, so no compiler is needed.

# ---- 1. Build the web UI ----------------------------------------------------------------
FROM node:22-slim AS web
WORKDIR /web
COPY frontend/package.json frontend/package-lock.json ./
# Optional build secret "ca": see the builder stage.
RUN --mount=type=secret,id=ca,required=false \
    if [ -f /run/secrets/ca ]; then export NODE_EXTRA_CA_CERTS=/run/secrets/ca; fi; \
    npm ci --no-audit --no-fund
COPY frontend/ ./
RUN npm run build

# ---- 2. Install the backend into a virtualenv from uv.lock ------------------------------
FROM python:3.12-slim AS builder
COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv
ENV UV_COMPILE_BYTECODE=1 UV_LINK_MODE=copy UV_PYTHON_DOWNLOADS=never UV_NO_CACHE=1
WORKDIR /app
COPY backend/pyproject.toml backend/uv.lock ./
# Dependencies first, so this layer is cached when only application code changes.
# Optional build secret "ca": a PEM CA bundle for networks that intercept TLS, used as
#   docker build --secret id=ca,src=/path/ca.pem .
RUN --mount=type=secret,id=ca,required=false \
    if [ -f /run/secrets/ca ]; then export SSL_CERT_FILE=/run/secrets/ca; fi; \
    uv sync --frozen --no-dev --no-install-project
COPY backend/stellarator ./stellarator
RUN --mount=type=secret,id=ca,required=false \
    if [ -f /run/secrets/ca ]; then export SSL_CERT_FILE=/run/secrets/ca; fi; \
    uv sync --frozen --no-dev --no-editable

# ---- 3. Runtime -------------------------------------------------------------------------
FROM python:3.12-slim
RUN useradd --system --uid 10001 --create-home --home-dir /home/stellarator stellarator \
 && mkdir /data && chown stellarator:stellarator /data
COPY --from=builder /app/.venv /app/.venv
COPY --from=web /web/dist /app/static
COPY tools/mock_server.py /app/tools/mock_server.py
ENV PATH="/app/.venv/bin:$PATH" \
    PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    STELLARATOR_DATA_DIR=/data \
    STELLARATOR_STATIC_DIR=/app/static
USER stellarator
WORKDIR /app
VOLUME /data
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
  CMD ["python", "-c", "import urllib.request,sys; sys.exit(0 if urllib.request.urlopen('http://127.0.0.1:8080/api/health', timeout=4).status == 200 else 1)"]
# One worker only: the run engine keeps run state (queues, concurrency, live counters) in this
# process. A second worker would not see runs started on the first.
CMD ["uvicorn", "stellarator.main:app", "--host", "0.0.0.0", "--port", "8080", "--workers", "1"]
