# Docker Orchestration Guide

## Development

```bash
make up          # Start all services
make down        # Stop all services
make logs        # View live logs
make ps          # List running containers
make reset-db    # Reset database volume
```

Or use docker-compose directly:
```bash
docker compose up
```

## Production

```bash
docker compose -f docker-compose.yml -f docker-compose.prod.yml up -d
```

This uses production configuration with:
- No source code mounts
- Resource limits enforced
- Production frontend build (`npm run build && npm start`)
- `restart: always` policy

## Data

SQLite database and application data stored in `stellarator-data` volume mounted at `/data`:
- Backend: `/data`
- Supervisor: `/data`

To reset: `make reset-db` or `docker volume rm stellarator-data`

## Observability

Enable Prometheus + Grafana (commented out by default):

```bash
docker compose --profile observability up
```

Access:
- Prometheus: http://localhost:9090
- Grafana: http://localhost:3001 (admin/admin)

**To enable metrics on backend:**
The backend includes prometheus-fastapi-instrumentator support. Add to `backend/app/main.py`:

```python
from prometheus_fastapi_instrumentator import Instrumentator

Instrumentator().instrument(app).expose(app)
```

This exposes `/metrics` endpoint for Prometheus scraping.

## Health Checks

- Backend: `curl http://localhost:8000/healthz`
- Supervisor: `curl http://localhost:8001/healthz`
- Frontend depends on backend health before starting

## Services

| Service | Port | Health Check |
|---------|------|--------------|
| Backend (FastAPI) | 8000 | `/healthz` |
| Supervisor | 8001 | `/healthz` |
| Frontend (Next.js) | 3000 | Depends on backend |
| Prometheus | 9090 | (observability only) |
| Grafana | 3001 | (observability only) |
