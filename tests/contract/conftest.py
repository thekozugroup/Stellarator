"""Session setup for the contract tests.

Everything the tests know about the server is HTTP: STELLARATOR_URL for the API under test and
MOCK_URL for the fake OpenAI-compatible server the API calls.

STELLARATOR_START chooses who starts them:
  (unset)  nothing is started; both URLs must already answer.
  python   `uvicorn stellarator.main:app` from backend/ and tools/mock_server.py, via uv.
  rust     target/debug/stellarator-server and target/debug/stellarator-mock (skip if not built).
The session always uses a fresh temp data dir when it starts the server.
"""

from __future__ import annotations

import os
import shutil
import signal
import socket
import subprocess
import tempfile
import time
from pathlib import Path
from types import SimpleNamespace
from urllib.parse import urlparse

import httpx
import pytest

from shapes import check, ENDPOINT, PROMPT, PROMPT_SET, RUN
from support import (
    assert_no_content,
    create_endpoint,
    create_run,
    get_run,
    import_prompts,
    jsonl,
    main_rows,
    source_file,
    source_paste,
    upload,
    wait_until,
)

REPO = Path(__file__).resolve().parents[2]
STELLARATOR_URL = os.environ.get("STELLARATOR_URL", "http://127.0.0.1:8155").rstrip("/")
MOCK_URL = os.environ.get("MOCK_URL", "http://127.0.0.1:9155/v1").rstrip("/")
START = os.environ.get("STELLARATOR_START", "").strip().lower()
TARGET_DIR = Path(os.environ.get("STELLARATOR_TARGET_DIR", REPO / "target" / "debug"))
STARTUP_TIMEOUT_S = float(os.environ.get("STELLARATOR_STARTUP_TIMEOUT", "180"))


def pytest_collection_modifyitems(items):
    for item in items:
        if "world" in getattr(item, "fixturenames", ()) or "main_run" in getattr(item, "fixturenames", ()):
            item.add_marker(pytest.mark.slow)


# --------------------------------------------------------------------------- starting servers


def _port_in_use(host: str, port: int) -> bool:
    with socket.socket() as s:
        s.settimeout(0.3)
        return s.connect_ex((host, port)) == 0


def _env(extra: dict[str, str]) -> dict[str, str]:
    env = dict(os.environ)
    # The servers talk to each other on loopback; never through a proxy.
    for var in ("NO_PROXY", "no_proxy"):
        hosts = [h for h in env.get(var, "").split(",") if h]
        for h in ("127.0.0.1", "localhost"):
            if h not in hosts:
                hosts.append(h)
        env[var] = ",".join(hosts)
    env.update(extra)
    return env


class _Procs:
    def __init__(self, logdir: Path):
        self.logdir = logdir
        self.procs: list[tuple[str, subprocess.Popen]] = []

    def start(self, name: str, cmd: list[str], cwd: Path, env: dict[str, str]) -> None:
        log = open(self.logdir / f"{name}.log", "wb")
        proc = subprocess.Popen(
            cmd, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True
        )
        self.procs.append((name, proc))

    def check_alive(self) -> None:
        for name, proc in self.procs:
            if proc.poll() is not None:
                tail = (self.logdir / f"{name}.log").read_text(errors="replace")[-1500:]
                pytest.exit(f"{name} exited with {proc.returncode} while starting.\n{tail}", returncode=2)

    def stop(self) -> None:
        for _, proc in reversed(self.procs):
            if proc.poll() is None:
                try:
                    os.killpg(proc.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
        deadline = time.monotonic() + 10
        for _, proc in self.procs:
            try:
                proc.wait(max(0.1, deadline - time.monotonic()))
            except subprocess.TimeoutExpired:
                try:
                    os.killpg(proc.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass


def _wait_http(url: str, what: str, procs: _Procs | None) -> None:
    deadline = time.monotonic() + STARTUP_TIMEOUT_S
    with httpx.Client(trust_env=False, timeout=2) as c:
        while True:
            try:
                if c.get(url).status_code == 200:
                    return
            except httpx.HTTPError:
                pass
            if procs:
                procs.check_alive()
            if time.monotonic() > deadline:
                pytest.exit(f"{what} did not answer at {url} within {STARTUP_TIMEOUT_S:.0f}s.", returncode=2)
            time.sleep(0.3)


@pytest.fixture(scope="session")
def servers():
    """Start (or just check) the API and mock servers. Yields a namespace with the data dir."""
    api_u, mock_u = urlparse(STELLARATOR_URL), urlparse(MOCK_URL)
    info = SimpleNamespace(data_dir=None, started=START, logdir=None)
    procs: _Procs | None = None
    tmp: Path | None = None
    try:
        if START in ("python", "rust"):
            if START == "rust":
                server_bin, mock_bin = TARGET_DIR / "stellarator-server", TARGET_DIR / "stellarator-mock"
                missing = [str(p) for p in (server_bin, mock_bin) if not p.is_file()]
                if missing:
                    pytest.skip(
                        "STELLARATOR_START=rust but not built: " + ", ".join(missing) +
                        ". Run `cargo build -p stellarator-server -p stellarator-mock` first."
                    )
            for u in (api_u, mock_u):
                if _port_in_use(u.hostname or "127.0.0.1", u.port or 80):
                    pytest.exit(
                        f"Port {u.port} is already in use, so STELLARATOR_START={START} would test "
                        f"whatever is listening there. Stop it or choose other URLs.",
                        returncode=2,
                    )
            tmp = Path(tempfile.mkdtemp(prefix="stellarator-contract-")).resolve()
            data_dir = tmp / "data"
            data_dir.mkdir()
            info.data_dir, info.logdir = data_dir, tmp
            procs = _Procs(tmp)
            api_port, mock_port = str(api_u.port), str(mock_u.port)
            if START == "python":
                procs.start(
                    "backend",
                    ["uv", "run", "uvicorn", "stellarator.main:app", "--port", api_port],
                    REPO / "backend",
                    _env({"STELLARATOR_DATA_DIR": str(data_dir)}),
                )
                procs.start(
                    "mock",
                    ["uv", "run", "--project", "backend", "python", "tools/mock_server.py",
                     "--port", mock_port, "--tps", "200"],
                    REPO,
                    _env({}),
                )
            else:
                procs.start(
                    "server",
                    [str(server_bin)],
                    REPO,
                    _env({"STELLARATOR_DATA_DIR": str(data_dir),
                          "STELLARATOR_BIND": f"{api_u.hostname}:{api_u.port}"}),
                )
                procs.start("mock", [str(mock_bin), "--port", mock_port, "--tps", "200"], REPO, _env({}))
        _wait_http(f"{MOCK_URL}/models", "The mock OpenAI server", procs)
        _wait_http(f"{STELLARATOR_URL}/api/health", "Stellarator", procs)
        yield info
    finally:
        if procs:
            procs.stop()
            if os.environ.get("STELLARATOR_KEEP_LOGS") and tmp:
                print(f"\nServer logs kept in {tmp}")
            elif tmp:
                shutil.rmtree(tmp, ignore_errors=True)


@pytest.fixture(scope="session")
def api(servers) -> httpx.Client:
    with httpx.Client(base_url=STELLARATOR_URL, timeout=60, trust_env=False) as client:
        yield client


@pytest.fixture(scope="session")
def mock_url(servers) -> str:
    return MOCK_URL


@pytest.fixture(scope="session")
def mock_root(servers) -> str:
    """The mock's address without the /v1 suffix."""
    return MOCK_URL.removesuffix("/v1")


# --------------------------------------------------------------------------- per-test factories


class Trash:
    """Resources a test created; removed afterwards (runs first, so nothing is in use)."""

    def __init__(self, api: httpx.Client):
        self.api = api
        self.runs: list[int] = []
        self.pref_sets: list[int] = []
        self.prompt_sets: list[int] = []
        self.endpoints: list[int] = []

    def empty(self) -> None:
        for kind, ids in (
            ("runs", self.runs),
            ("preference-sets", self.pref_sets),
            ("prompt-sets", self.prompt_sets),
            ("endpoints", self.endpoints),
        ):
            for i in reversed(ids):
                try:
                    self.api.delete(f"/api/{kind}/{i}")
                except httpx.HTTPError:
                    pass


@pytest.fixture
def trash(api):
    t = Trash(api)
    yield t
    t.empty()


@pytest.fixture
def make_endpoint(api, mock_url, trash):
    def make(base_url: str | None = None, **kw):
        ep = create_endpoint(api, base_url or mock_url, **kw)
        trash.endpoints.append(ep["id"])
        return ep

    return make


@pytest.fixture
def make_prompt_set(api, trash):
    """Paste-import `n` one-line prompts (or the given text) and return the prompt set."""

    def make(n: int = 5, name: str | None = None, text: str | None = None, mapping: dict | None = None):
        body = text if text is not None else "\n".join(f"Say the number {i} in words, set {time.time_ns()}" for i in range(n))
        mapping = mapping if mapping is not None else {"split_mode": "line"}
        ps = import_prompts(api, source_paste(body), name or f"Prompts {time.time_ns()}", mapping)
        trash.prompt_sets.append(ps["id"])
        return ps

    return make


@pytest.fixture
def make_run(api, trash):
    def make(endpoint_id: int, prompt_set_id: int, model: str = "mock-plain", **kw):
        run = create_run(api, endpoint_id, prompt_set_id, model, **kw)
        trash.runs.append(run["id"])
        return run

    return make


@pytest.fixture
def hf_token_cleared(api):
    """Make sure no Hugging Face token is saved (skip when the server gets one from the environment)."""
    r = api.put("/api/settings", json={"hf_token": ""})
    assert r.status_code == 200, r.text
    if r.json()["hf_token_from_env"]:
        pytest.skip("The server has HF_TOKEN in its environment, so 'no token' cannot be tested.")
    yield
    api.put("/api/settings", json={"hf_token": ""})


# --------------------------------------------------------------------------- the shared world
#
# One prompt set (30 chat prompts with a system message) and four finished runs over it, created
# together so they generate in parallel:
#   main   mock-reasoning-content, 2 samples per prompt (60 samples), live snapshots recorded
#   think  mock-think-tags, 1 sample (thinking split out of the content by the tags)
#   plain  mock-plain, 1 sample (no reasoning)
#   trunc  mock-truncating, 1 sample (every sample stops with finish_reason "length")
# plus a second, tiny prompt set with one run, for "different prompt set" cases.

N_PROMPTS = 30


@pytest.fixture(scope="session")
def world(api, mock_url):
    ep = create_endpoint(api, mock_url, name="Contract mock")
    ps_rows = main_rows(N_PROMPTS)
    up = upload(api, "contract-prompts.jsonl", jsonl(ps_rows), "application/x-ndjson")
    ps = import_prompts(api, source_file(up["upload_id"]), "Contract prompts")
    prompts = []
    offset = 0
    while True:
        page = api.get(f"/api/prompt-sets/{ps['id']}/prompts", params={"offset": offset, "limit": 500}).json()
        prompts += page["items"]
        offset += len(page["items"])
        if offset >= page["total"] or not page["items"]:
            break
    assert len(prompts) == N_PROMPTS

    other_ps = import_prompts(
        api, source_paste("Alpha one\nBeta two\nGamma three"), "Contract other prompts", {"split_mode": "line"}
    )

    created = {
        "main": create_run(api, ep["id"], ps["id"], "mock-reasoning-content", name="Contract Main Run", samples_per_prompt=2),
        "think": create_run(api, ep["id"], ps["id"], "mock-think-tags", name="Contract Think Run"),
        "plain": create_run(api, ep["id"], ps["id"], "mock-plain", name="Contract Plain Run"),
        "trunc": create_run(api, ep["id"], ps["id"], "mock-truncating", name="Contract Truncated Run"),
    }
    other_created = create_run(api, ep["id"], other_ps["id"], "mock-plain", name="Contract Other Run")

    # Watch the main run while everything generates.
    live_snapshots: list[dict] = []
    pending = {k: v["id"] for k, v in created.items()}
    pending["other"] = other_created["id"]
    final: dict[str, dict] = {}
    deadline = time.monotonic() + 150
    while pending:
        if "main" in pending:
            r = api.get(f"/api/runs/{pending['main']}/live")
            assert r.status_code == 200, r.text
            live_snapshots.append(r.json())
        for key, run_id in list(pending.items()):
            run = get_run(api, run_id)
            if run["status"] == "completed":
                final[key] = run
                del pending[key]
        if pending:
            assert time.monotonic() < deadline, f"runs did not finish: {pending}"
            time.sleep(0.12)
    ns = SimpleNamespace(
        endpoint=ep,
        prompt_set=ps,
        prompts=prompts,
        other_prompt_set=other_ps,
        created=created,
        other_created=other_created,
        main=final["main"],
        think=final["think"],
        plain=final["plain"],
        trunc=final["trunc"],
        other=final["other"],
        live_snapshots=live_snapshots,
        runs_on_endpoint=5,
        runs_on_prompt_set=4,
    )
    yield ns
    for run in (ns.main, ns.think, ns.plain, ns.trunc, ns.other):
        api.delete(f"/api/runs/{run['id']}")
    for pid in (ps["id"], other_ps["id"]):
        api.delete(f"/api/prompt-sets/{pid}")
    api.delete(f"/api/endpoints/{ep['id']}")


@pytest.fixture(scope="session")
def main_run(world):
    return world.main
