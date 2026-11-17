# Contract tests

Black-box HTTP tests for the Stellarator API. They check status codes, response shapes (every field,
its type and its enum values), error bodies (`{"detail", "fields"}`) and behaviour across every
route, talking only HTTP to the server and to a mock OpenAI-compatible server.

They were written against the Python backend the Rust server replaced and passed on both before the
Python code was removed (184 shared tests), which is how the port was checked.

## Run

Needs [uv](https://docs.astral.sh/uv/) and Python 3.12. From the repo root:

```bash
cargo build -p stellarator-server -p stellarator-mock
```

```bash
STELLARATOR_START=rust uv run --project tests/contract pytest tests/contract -m "not v1"
```

Against servers you started yourself:

```bash
STELLARATOR_URL=http://127.0.0.1:8080 MOCK_URL=http://127.0.0.1:9100/v1 \
  uv run --project tests/contract pytest tests/contract -m "not v1"
```

A full run takes about 35 seconds.

## Settings

| Variable | Default | Meaning |
| --- | --- | --- |
| `STELLARATOR_URL` | `http://127.0.0.1:8155` | The API under test |
| `MOCK_URL` | `http://127.0.0.1:9155/v1` | The mock OpenAI server, as the API reaches it |
| `STELLARATOR_START` | unset | `rust`: start `target/debug/stellarator-server` and `target/debug/stellarator-mock` with a fresh temp data dir. Unset: use what is running |
| `STELLARATOR_STARTUP_TIMEOUT` | `180` | Seconds to wait for started servers |
| `STELLARATOR_KEEP_LOGS` | unset | Keep `server.log` and `mock.log` from a started session |

The session refuses to start servers on ports that are already in use, so it never tests a stray
server by accident.

## Markers

| Marker | What | Deselect with |
| --- | --- | --- |
| `v2` | Behaviour added with the Rust server: endpoint presets, background imports (202, `importing` to `ready`), prompt set `status` fields, `ImportMapping.turns`, `has_conversations` and `dropped` in previews, `SettingsOut.mode` | (always runs) |
| `v1` | Python-only behaviour kept for reference (`POST /api/imports` answering 201 with the finished set) | `-m "not v1"` |

Not covered: Hugging Face imports and a real Hub push (network), and restart recovery (the tests only
speak HTTP). The Rust crates test those themselves.
