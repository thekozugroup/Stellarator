"""Small helpers shared by everything that talks to an OpenAI-compatible server."""

from __future__ import annotations

import time

import httpx


def api_root(base_url: str) -> str:
    """Normalize a base URL to the API root that `/chat/completions` and `/models` hang off.

    Accepts "http://host:8000", "http://host:8000/v1", "https://api.example.com/v1/" or a URL that
    already ends in "/chat/completions". A URL without any path gets "/v1" appended, which is what
    vLLM, SGLang, llama.cpp, Ollama, LM Studio and most hosted APIs expect.
    """
    url = base_url.strip().rstrip("/")
    for suffix in ("/chat/completions", "/completions", "/models"):
        if url.endswith(suffix):
            url = url[: -len(suffix)]
    parsed = httpx.URL(url)
    if parsed.path in ("", "/"):
        url = url + "/v1"
    return url


def build_headers(api_key: str | None, extra: dict[str, str] | None = None) -> dict[str, str]:
    headers = {"Content-Type": "application/json"}
    if api_key:
        headers["Authorization"] = f"Bearer {api_key}"
    if extra:
        headers.update(extra)
    return headers


def describe_http_error(exc: Exception) -> str:
    """One sentence a person can act on."""
    if isinstance(exc, httpx.ConnectError):
        return "Could not connect. Check the URL and that the server is up."
    if isinstance(exc, httpx.ConnectTimeout):
        return "Connection timed out. Check the URL and network."
    if isinstance(exc, httpx.TimeoutException):
        return "The server took too long to answer."
    if isinstance(exc, httpx.HTTPStatusError):
        status = exc.response.status_code
        body = exc.response.text[:300].strip()
        if status == 401:
            return "The server rejected the API key (401)."
        if status == 403:
            return "The API key is not allowed to do this (403)."
        if status == 404:
            return "Nothing found at this URL (404). Does the base URL end in /v1?"
        return f"The server answered {status}: {body}" if body else f"The server answered {status}."
    return str(exc) or exc.__class__.__name__


async def list_models(
    base_url: str, api_key: str | None, headers: dict[str, str] | None = None, timeout: float = 15
) -> tuple[list[str], int]:
    """GET {root}/models. Returns (sorted model ids, latency in ms). Raises httpx errors."""
    start = time.perf_counter()
    async with httpx.AsyncClient(timeout=timeout) as client:
        resp = await client.get(f"{api_root(base_url)}/models", headers=build_headers(api_key, headers))
        resp.raise_for_status()
        data = resp.json()
    latency = int((time.perf_counter() - start) * 1000)
    items = data.get("data", data) if isinstance(data, dict) else data
    ids: list[str] = []
    if isinstance(items, list):
        for item in items:
            if isinstance(item, dict) and item.get("id"):
                ids.append(str(item["id"]))
            elif isinstance(item, str):
                ids.append(item)
    # Ollama's native shape: {"models": [{"name": ...}]}
    if not ids and isinstance(data, dict) and isinstance(data.get("models"), list):
        ids = [str(m.get("name") or m.get("model")) for m in data["models"] if isinstance(m, dict)]
    return sorted(set(ids), key=str.lower), latency


async def check_key(
    base_url: str, api_key: str | None, headers: dict[str, str] | None = None, timeout: float = 15
) -> str | None:
    """Return a sentence when the server rejects the API key, else None.

    Some servers (OpenRouter, for one) list models without checking the key, so a working
    GET /models proves nothing about it. This sends an empty chat request instead: servers check
    the key before they read the body, so 401 or 403 means the key was rejected, and any other
    answer (400, 404, 422) means it was accepted. Nothing is generated; max_tokens is 1 in case a
    lenient server tries anyway. Network errors return None: the models check reports those.
    """
    try:
        async with httpx.AsyncClient(timeout=timeout) as client:
            resp = await client.post(
                f"{api_root(base_url)}/chat/completions",
                headers=build_headers(api_key, headers),
                json={"model": "", "messages": [], "max_tokens": 1},
            )
    except httpx.HTTPError:
        return None
    if resp.status_code in (401, 403):
        sentence = describe_http_error(httpx.HTTPStatusError("", request=resp.request, response=resp))
        detail = _server_message(resp)
        return f"{sentence[:-1]}: {detail}" if detail and sentence.endswith(".") else sentence
    return None


def _server_message(resp: httpx.Response) -> str | None:
    try:
        body = resp.json()
    except ValueError:
        return None
    err = body.get("error") if isinstance(body, dict) else None
    msg = err.get("message") if isinstance(err, dict) else (err if isinstance(err, str) else None)
    msg = (msg or (body.get("message") if isinstance(body, dict) else None) or "").strip()
    return msg[:160] or None
