# Stellarator API contract tests

Black-box HTTP tests for the Stellarator API. They run against the Python backend (the reference)
and the Rust backend (the port) and check status codes, response shapes (every field, its type and
enums), `{detail, fields}` error bodies and behaviour: endpoints, prompt sets and imports, runs
(generation, live view, pause/resume, retry, delete), exports in every format and file type,
preferences, settings.

The tests only speak HTTP. They call the API at `STELLARATOR_URL` and rely on a mock
OpenAI-compatible server at `MOCK_URL`, which the API server must be able to reach.

## Run

Needs [uv](https://docs.astral.sh/uv/) and Python 3.12. From the repo root:

```bash
# Python backend: starts uvicorn (backend/) and tools/mock_server.py itself
STELLARATOR_START=python uv run --project tests/contract pytest tests/contract -m "not v2"

# Rust backend: starts target/debug/stellarator-server and target/debug/stellarator-mock
cargo build -p stellarator-server -p stellarator-mock
STELLARATOR_START=rust uv run --project tests/contract pytest tests/contract -m "not v1"

# Servers you started yourself (any backend)
STELLARATOR_URL=http://127.0.0.1:8155 MOCK_URL=http://127.0.0.1:9155/v1 \
  uv run --project tests/contract pytest tests/contract
```

Or `cd tests/contract && uv run pytest ...`. A full run takes about 40 seconds.

| Variable | Default | Meaning |
| --- | --- | --- |
| `STELLARATOR_URL` | `http://127.0.0.1:8155` | API under test |
| `MOCK_URL` | `http://127.0.0.1:9155/v1` | Mock OpenAI server (include `/v1`) |
| `STELLARATOR_START` | unset | `python` or `rust`: start the servers (below). Unset: use what is running |
| `STELLARATOR_TARGET_DIR` | `<repo>/target/debug` | Where the Rust binaries are |
| `STELLARATOR_STARTUP_TIMEOUT` | `180` | Seconds to wait for started servers (first `uv run` may sync) |
| `STELLARATOR_KEEP_LOGS` | unset | Keep the temp dir with the server logs after the run |

With `STELLARATOR_START` set, the session fixture:

- refuses to run when the API or mock port is already taken, so it never tests a stray server;
- creates a fresh temp data dir and passes it as `STELLARATOR_DATA_DIR`;
- `python`: `cd backend && uv run uvicorn stellarator.main:app --port <port of STELLARATOR_URL>` and
  `uv run --project backend python tools/mock_server.py --port <mock port> --tps 200`;
- `rust`: `stellarator-server` with `STELLARATOR_BIND=<host:port of STELLARATOR_URL>` and
  `stellarator-mock --port <mock port> --tps 200`; if either binary is missing the whole session is
  skipped with a message saying which;
- stops both (process groups) and deletes the temp dir at the end. Logs are in
  `<tmp>/backend.log`, `server.log` and `mock.log` (set `STELLARATOR_KEEP_LOGS=1` to keep them).

## Markers

| Marker | Meaning | Selection |
| --- | --- | --- |
| `v2` | Behaviour new in the Rust API: endpoint presets, background imports (202, `importing` to `ready`), prompt set `status` fields, `ImportMapping.turns`, `has_conversations` and `dropped` in previews, `SettingsOut.mode` | Python run: `-m "not v2"` |
| `v1` | Python-only behaviour (`POST /api/imports` answers 201 with the finished set) | Rust run: `-m "not v1"` |
| `slow` | Waits for runs to generate (added automatically to tests that use the shared runs) | `-m "not slow"` for a quick pass |

`v2` tests fail on the Python backend by design; they collect and run, nothing more.

Tests that import prompts as setup (`support.import_prompts`) work on both backends: they accept 201
or 202 and poll `GET /api/prompt-sets/{id}` until the set is no longer `importing`. Shape checks
treat the v2 prompt set, column, preview and settings fields as optional, so a v1-era test passes on
either backend; the `v2` tests require them.

## Layout

| File | Contents |
| --- | --- |
| `conftest.py` | Server startup, the `api` client, per-test factories (`make_endpoint`, `make_prompt_set`, `make_run`, cleanup) and the shared `world` |
| `shapes.py` | Response shape specs (`check(value, spec)`), one per schema in `crates/core/src/schemas.rs` |
| `support.py` | Polling, `assert_error`, upload/import/run helpers |
| `exportkit.py` | Parses jsonl, json, csv and parquet downloads and compares them with each other |
| `test_*.py` | One file per area |

The `world` fixture (session scope) imports 30 chat prompts (system + user, one scalar column that
becomes `meta`) and runs four models over them at once, so the whole suite pays for generation once:
`mock-reasoning-content` with 2 samples per prompt (60 samples, watched through `/live` while it runs),
`mock-think-tags`, `mock-plain` and `mock-truncating`. Tests that change things (pause, flaky models,
deletes, templates) create their own endpoint, prompt set and run and delete them afterwards. The
database is shared for the whole session, so no test assumes an empty table.

## Adding a test

- Assert what the frontend relies on: status code, every field, error sentence for errors the user
  can fix. Sentences produced by Pydantic or serde (`Field required`, `Input should be ...`) differ
  between backends: assert the field name in `fields`, not the text.
- Use `check(body, SHAPE)` for response shapes. Add new fields to `shapes.py` (as `optional` while
  they exist only in v2).
- Mark v2-only behaviour `@pytest.mark.v2` and Python-only behaviour `@pytest.mark.v1`.
