"""FastAPI application: the JSON API under /api and the built web UI everywhere else."""

from __future__ import annotations

from contextlib import asynccontextmanager

from fastapi import FastAPI, HTTPException, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import FileResponse, JSONResponse
from fastapi.staticfiles import StaticFiles

from . import __version__
from .api import endpoints, exports, imports, preferences, prompt_sets, runs, settings
from .config import STATIC_DIR
from .db import init_db
from .services.engine import engine_registry


@asynccontextmanager
async def lifespan(_app: FastAPI):
    await init_db()
    await engine_registry.recover()
    yield
    await engine_registry.shutdown()


app = FastAPI(title="Stellarator", version=__version__, lifespan=lifespan)

for module in (endpoints, prompt_sets, imports, runs, exports, preferences, settings):
    app.include_router(module.router)


@app.exception_handler(RequestValidationError)
async def _validation_error(_request: Request, exc: RequestValidationError):
    """Return field errors as {"detail": str, "fields": {"a.b": "message"}} for the forms."""
    fields: dict[str, str] = {}
    for err in exc.errors():
        loc = [str(p) for p in err.get("loc", []) if p not in ("body", "query", "path")]
        msg = str(err.get("msg", "Invalid value")).removeprefix("Value error, ")
        fields[".".join(loc) or "_"] = msg
    first = next(iter(fields.values()), "Invalid request")
    return JSONResponse(status_code=422, content={"detail": first, "fields": fields})


@app.get("/api/health")
async def health():
    return {"ok": True, "version": __version__}


if STATIC_DIR.is_dir() and (STATIC_DIR / "index.html").is_file():
    assets = STATIC_DIR / "assets"
    if assets.is_dir():
        app.mount("/assets", StaticFiles(directory=assets), name="assets")

    @app.get("/{path:path}", include_in_schema=False)
    async def spa(path: str):
        if path.startswith("api/"):
            raise HTTPException(404, "Not found")
        candidate = (STATIC_DIR / path).resolve()
        if path and candidate.is_file() and STATIC_DIR.resolve() in candidate.parents:
            return FileResponse(candidate)
        return FileResponse(STATIC_DIR / "index.html", headers={"Cache-Control": "no-cache"})
