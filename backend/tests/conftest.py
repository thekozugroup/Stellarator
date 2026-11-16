"""Shared test setup.

- A throwaway data dir, set before `stellarator` is imported (config reads it at import time).
- `app` / `client`: the FastAPI app with its lifespan running, and an httpx client for it.
- `mock_server`: tools/mock_server.py on 127.0.0.1:9101 (override with STELLARATOR_TEST_MOCK_PORT),
  started once per session; yields its base URL ("http://127.0.0.1:9101/v1").
- `make_prompt_set`: inserts a prompt set with n prompts straight into the DB.

The DB is shared by every test in the session, so tests must not assume empty tables.
"""

from __future__ import annotations

import os
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

os.environ["STELLARATOR_DATA_DIR"] = tempfile.mkdtemp(prefix="stellarator-tests-")
os.environ.pop("STELLARATOR_DB_URL", None)
for _var in ("NO_PROXY", "no_proxy"):
    _hosts = os.environ.get(_var, "")
    if "127.0.0.1" not in _hosts.split(","):
        os.environ[_var] = ",".join(h for h in (_hosts, "localhost,127.0.0.1") if h)

import httpx  # noqa: E402
import pytest  # noqa: E402
import pytest_asyncio  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[2]
MOCK_PORT = int(os.environ.get("STELLARATOR_TEST_MOCK_PORT", "9101"))


@pytest_asyncio.fixture
async def app():
    """The app with startup (create tables, recover) and shutdown (stop runs) around the test."""
    from stellarator.db import engine
    from stellarator.main import app as fastapi_app
    from stellarator.main import lifespan

    async with lifespan(fastapi_app):
        yield fastapi_app
    await engine.dispose()


@pytest_asyncio.fixture
async def client(app):
    transport = httpx.ASGITransport(app=app)
    async with httpx.AsyncClient(transport=transport, base_url="http://test", timeout=60) as c:
        yield c


def _port_open(port: int) -> bool:
    with socket.socket() as s:
        s.settimeout(0.2)
        return s.connect_ex(("127.0.0.1", port)) == 0


@pytest.fixture(scope="session")
def mock_server():
    """Base URL of the mock OpenAI-compatible server (fast: 400 tokens/s per sequence)."""
    base = f"http://127.0.0.1:{MOCK_PORT}/v1"
    if _port_open(MOCK_PORT):  # already running (e.g. started by hand)
        yield base
        return
    proc = subprocess.Popen(
        [sys.executable, str(REPO_ROOT / "tools" / "mock_server.py"), "--host", "127.0.0.1",
         "--port", str(MOCK_PORT), "--tps", "400"],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        deadline = time.monotonic() + 20
        while not _port_open(MOCK_PORT):
            if proc.poll() is not None or time.monotonic() > deadline:
                raise RuntimeError(f"mock server did not start on port {MOCK_PORT}")
            time.sleep(0.1)
        yield base
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()


@pytest_asyncio.fixture
async def make_prompt_set(app):
    """Factory: await make_prompt_set(n, name=None, messages=None) -> prompt set id."""
    from stellarator.db import SessionLocal
    from stellarator.models import Prompt, PromptSet

    async def make(n: int, name: str | None = None, messages=None) -> int:
        async with SessionLocal() as session:
            ps = PromptSet(name=name or f"Test set ({n})", source_kind="paste", source={}, prompt_count=n)
            session.add(ps)
            await session.flush()
            rows = [
                {
                    "prompt_set_id": ps.id,
                    "idx": i,
                    "messages": messages(i) if callable(messages) else
                    [{"role": "user", "content": f"Question {i}: what is {i} + {i}?"}],
                    "meta": {},
                }
                for i in range(n)
            ]
            if rows:
                await session.execute(Prompt.__table__.insert(), rows)
            await session.commit()
            return ps.id

    return make
