# Running Stellarator in Docker

Stellarator ships as one image (`stellarator:latest`): a single Rust binary that serves the web UI
and the API on container port 8080, on a Debian slim base, running as a non-root user. It has no
sign-in. Do not expose it to the internet.

## Quick start

```sh
git clone <repo-url> Stellarator && cd Stellarator
docker compose up -d --build
```

Open http://localhost:8080. Set `STELLARATOR_PORT` to use another host port:

```sh
STELLARATOR_PORT=8108 docker compose up -d
```

Check it: `curl localhost:8080/api/health` returns `{"ok":true,"version":"..."}`.

To try it without a GPU, start the mock model server too and add an endpoint with base URL
`http://mock:9100/v1`:

```sh
docker compose --profile mock up -d
```

## DGX Spark and other model servers on the same machine

The container reaches the host as `host.docker.internal` (compose maps it to the host gateway).
Use these base URLs when you add an endpoint:

| Server | Base URL |
| --- | --- |
| vLLM (`--port 8000`) | `http://host.docker.internal:8000/v1` |
| SGLang (`--port 30000`) | `http://host.docker.internal:30000/v1` |
| llama.cpp `llama-server` (`--port 8080`) | `http://host.docker.internal:8080/v1` |
| Ollama | `http://host.docker.internal:11434/v1` |
| NVIDIA NIM, TensorRT-LLM `trtllm-serve` | OpenAI-compatible; use the port you published, same pattern |

The model server must listen on `0.0.0.0` (vLLM and SGLang: `--host 0.0.0.0`). A server bound to
`127.0.0.1` is not reachable through `host.docker.internal`. If you cannot rebind it, use host
networking (below). A model server in another container: put both on one Docker network and use
its service name, for example `http://vllm:8000/v1`.

The image is multi-arch. The DGX Spark is linux/arm64 and builds natively with
`docker compose build`; no flags are needed.

## Network modes

- **Bridge (default).** Stellarator is published on `STELLARATOR_PORT`. Model servers on the host
  are reached at `host.docker.internal`.
- **Host network.** In `docker-compose.yml`, remove `ports` and `extra_hosts` from the
  `stellarator` service and uncomment `network_mode: host`. The app then listens on host port
  8080 (`STELLARATOR_PORT` is ignored) and reaches a server bound to `127.0.0.1` at
  `http://127.0.0.1:8000/v1`. Use this only when the model server cannot listen on `0.0.0.0`.
  Host networking works on Linux only.

## Environment variables

| Variable | Default | Meaning |
| --- | --- | --- |
| `STELLARATOR_PORT` | `8080` | Host port (compose only). Container port is always 8080. |
| `HF_TOKEN` | empty | Hugging Face token for gated or private datasets and for Hub export. Read from your shell or a `.env` file next to `docker-compose.yml`. A token saved in the app settings is used only when this is empty. |
| `STELLARATOR_DATA_DIR` | `/data` | Where the SQLite database and uploads live. Declared as a volume. |
| `STELLARATOR_BIND` | `0.0.0.0:8080` | Address the server listens on inside the container. |
| `RUST_LOG` | `info` | Log level (`warn` for quieter logs, `debug` to trace requests). |

## Data, backup and restore

All state is in the named volume `stellarator-data`, mounted at `/data`. Stop the app first so the
database is not written during the copy.

```sh
docker compose stop stellarator

# back up
docker run --rm -v stellarator-data:/data -v "$PWD":/backup alpine \
  tar czf /backup/stellarator-data.tgz -C /data .

# restore (into an empty volume)
docker run --rm -v stellarator-data:/data -v "$PWD":/backup alpine \
  sh -c 'tar xzf /backup/stellarator-data.tgz -C /data && chown -R 10001:10001 /data'

docker compose start stellarator
```

Compose prefixes volume names with the project name, so the volume may be called
`stellarator_stellarator-data`; check with `docker volume ls`. `docker compose down` keeps the
volume. `docker compose down -v` deletes it and all runs.

The container runs as UID 10001. When you bind-mount a host directory at `/data` instead of the
volume, make it writable by that UID.

## Upgrading

```sh
git pull
docker compose up -d --build
```

The volume is kept and the database is migrated on start. Back up first if the run data matters.
Runs that were in progress when the container stopped are recovered on start.

## Notes

- Run one container per data volume. The run engine and import jobs keep live state in the
  process, so do not scale the service.
- The health check is the binary itself (`stellarator-server healthcheck`); the image has no curl.
- A data volume made by the earlier Python image opens as is: the database is upgraded in place.
- Stopping the container pauses in-flight requests; check the run page after a restart.
- Behind a TLS-intercepting proxy, pass its CA bundle at build time:
  `docker buildx build --secret id=ca,src=/path/ca.pem -t stellarator:latest .`
- Cross-build for another architecture: `docker buildx build --platform linux/arm64 -t stellarator:latest .`
  (needs QEMU/binfmt on the build machine).
