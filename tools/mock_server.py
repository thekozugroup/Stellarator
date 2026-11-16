"""A fake OpenAI-compatible server for trying Stellarator without a GPU.

    uv run --project backend python tools/mock_server.py --port 9100

Then add an endpoint with base URL http://localhost:9100/v1 (or http://host.docker.internal:9100/v1
from inside Docker). Each model returns its thinking trace a different way:

    mock-reasoning-content   delta.reasoning_content (vLLM < 0.10, SGLang, DeepSeek API)
    mock-reasoning           delta.reasoning (vLLM >= 0.10, OpenRouter, Ollama)
    mock-think-tags          inline <think>...</think> in content (no reasoning parser)
    mock-open-think          content starts mid-thought and only has </think> (template opened it)
    mock-plain               no thinking at all
    mock-flaky               like mock-reasoning-content, but 20% of requests fail with 500 or 429
    mock-truncating          stops with finish_reason "length"

Speed: --tps tokens per second per sequence (default 60). Failures and delays are random.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import random
import time

import uvicorn
from fastapi import FastAPI, Request
from fastapi.responses import JSONResponse, StreamingResponse

MODELS = [
    "mock-reasoning-content",
    "mock-reasoning",
    "mock-think-tags",
    "mock-open-think",
    "mock-plain",
    "mock-flaky",
    "mock-truncating",
]

app = FastAPI()
TPS = 60.0


def _last_user(messages: list[dict]) -> str:
    for m in reversed(messages):
        if m.get("role") == "user":
            c = m.get("content")
            return c if isinstance(c, str) else json.dumps(c)
    return ""


def _compose(prompt: str, rng: random.Random) -> tuple[list[str], list[str]]:
    """Return (thinking words, answer words)."""
    head = " ".join(prompt.split()[:12])
    thinking = (
        f"The question asks: {head}. Let me break it down. First I identify what is given and what "
        f"is asked. Then I work through each step and check the result against the constraints. "
    )
    thinking += " ".join(
        rng.choice(
            [
                "Checking the arithmetic again.",
                "That matches the earlier step.",
                "Another approach would be to work backwards.",
                "So the intermediate value holds.",
                "I should double-check the units.",
                "Combining these gives the total.",
            ]
        )
        for _ in range(rng.randint(6, 24))
    )
    answer = (
        f"Here is the answer to “{head}”. Working step by step, the key quantities combine to give "
        f"the result. Final answer: {rng.randint(2, 999)}."
    )
    return thinking.split(" "), answer.split(" ")


def _chunk(model: str, delta: dict, finish: str | None = None, usage: dict | None = None) -> str:
    obj = {
        "id": "chatcmpl-mock",
        "object": "chat.completion.chunk",
        "created": int(time.time()),
        "model": model,
        "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
    }
    if usage is not None:
        obj["usage"] = usage
        obj["choices"] = []
    return f"data: {json.dumps(obj)}\n\n"


@app.get("/v1/models")
async def models():
    return {"object": "list", "data": [{"id": m, "object": "model", "owned_by": "mock"} for m in MODELS]}


@app.post("/v1/chat/completions")
async def chat(request: Request):
    body = await request.json()
    model = body.get("model", "")
    if model not in MODELS:
        return JSONResponse({"error": {"message": f"The model `{model}` does not exist."}}, status_code=404)
    rng = random.Random()
    if model == "mock-flaky" and rng.random() < 0.2:
        if rng.random() < 0.5:
            return JSONResponse({"error": {"message": "Rate limited"}}, status_code=429, headers={"Retry-After": "1"})
        return JSONResponse({"error": {"message": "Internal error"}}, status_code=500)

    prompt = _last_user(body.get("messages", []))
    think, answer = _compose(prompt, rng)
    max_tokens = body.get("max_tokens") or body.get("max_completion_tokens") or 100000
    finish = "stop"
    if model == "mock-truncating":
        max_tokens = min(max_tokens, len(think) // 2)
    if model == "mock-plain":
        think = []

    # Build the token stream as (field, text) pairs.
    tokens: list[tuple[str, str]] = []
    field = {"mock-reasoning-content": "reasoning_content", "mock-flaky": "reasoning_content"}.get(model, "reasoning")
    if model in ("mock-think-tags", "mock-open-think"):
        if model == "mock-think-tags":
            tokens.append(("content", "<think>\n"))
        tokens += [("content", w + " ") for w in think]
        tokens.append(("content", "\n</think>\n\n"))
    else:
        tokens += [(field, w + " ") for w in think]
    tokens += [("content", w + ("" if i == len(answer) - 1 else " ")) for i, w in enumerate(answer)]
    if len(tokens) > max_tokens:
        tokens = tokens[:max_tokens]
        finish = "length"

    prompt_tokens = max(1, len(prompt.split()))
    reasoning_tokens = sum(1 for f, _ in tokens if f != "content") or None
    usage = {
        "prompt_tokens": prompt_tokens,
        "completion_tokens": len(tokens),
        "total_tokens": prompt_tokens + len(tokens),
    }
    if reasoning_tokens:
        usage["completion_tokens_details"] = {"reasoning_tokens": reasoning_tokens}

    await asyncio.sleep(rng.uniform(0.05, 0.4))  # queueing / prefill

    if not body.get("stream"):
        await asyncio.sleep(len(tokens) / TPS)
        msg: dict = {"role": "assistant", "content": "".join(t for f, t in tokens if f == "content")}
        r = "".join(t for f, t in tokens if f != "content")
        if r:
            msg[field] = r
        return {
            "id": "chatcmpl-mock",
            "object": "chat.completion",
            "created": int(time.time()),
            "model": model,
            "choices": [{"index": 0, "message": msg, "finish_reason": finish}],
            "usage": usage,
        }

    include_usage = (body.get("stream_options") or {}).get("include_usage")

    async def gen():
        yield _chunk(model, {"role": "assistant", "content": ""})
        for f, t in tokens:
            await asyncio.sleep(rng.expovariate(TPS))
            yield _chunk(model, {f: t})
        yield _chunk(model, {}, finish=finish)
        if include_usage:
            yield _chunk(model, {}, usage=usage)
        yield "data: [DONE]\n\n"

    return StreamingResponse(gen(), media_type="text/event-stream")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--host", default="0.0.0.0")
    parser.add_argument("--port", type=int, default=9100)
    parser.add_argument("--tps", type=float, default=60.0)
    args = parser.parse_args()
    TPS = args.tps
    uvicorn.run(app, host=args.host, port=args.port, log_level="warning")
