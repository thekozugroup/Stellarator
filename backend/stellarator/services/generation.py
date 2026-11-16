"""One chat-completion request: build it, send it (streaming or not), retry it, and split the
thinking trace from the answer.

Public pieces:
- `prepare_messages(messages, params)`  the prompt as sent (system prompt and user template applied)
- `build_body(model, messages, params, sample_idx, ...)`  the JSON request body
- `split_inline(text, think_open, think_close)`  thinking/answer split for inline tags and harmony
- `generate(client, url, model, messages, params, sample_idx, ...)`  one sample, with retries
- `Progress`, `ServerCaps`, `GenerationResult`, `GenerationFailed`
"""

from __future__ import annotations

import asyncio
import email.utils
import json
import random
import re
import time
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any

import httpx

from ..schemas import RunParams
from .openai_compat import describe_http_error

RETRY_STATUSES = frozenset({408, 409, 425, 429, 500, 502, 503, 504})
TAIL_CHARS = 240
MAX_BACKOFF_S = 30.0
MAX_RETRY_AFTER_S = 300.0
REASONING_FIELDS = ("reasoning_content", "reasoning", "thinking")

_HARMONY_SEGMENT = re.compile(
    r"<\|channel\|>([^<]*?)(?:<\|constrain\|>[^<]*)?<\|message\|>(.*?)"
    r"(?=<\|end\|>|<\|return\|>|<\|call\|>|<\|start\|>|<\|channel\|>|\Z)",
    re.S,
)
_HARMONY_FINAL_MARK = "<|channel|>final"


# --------------------------------------------------------------------------- data


@dataclass
class Progress:
    """What one request is doing right now, for the live view."""

    phase: str = "waiting"  # waiting | thinking | answering
    tokens: int = 0  # streamed chunks that carried text in this attempt
    tail: str = ""  # last ~240 characters of the current phase
    attempt: int = 1


@dataclass
class ServerCaps:
    """What we learned about a server during a run (shared by every request of the run)."""

    stream_options: bool = True


@dataclass
class GenerationResult:
    reasoning: str | None
    response: str
    finish_reason: str | None
    prompt_tokens: int | None
    completion_tokens: int | None
    reasoning_tokens: int | None
    latency_ms: int
    ttft_ms: int | None
    attempts: int
    chunks: int  # text-carrying chunks streamed in the successful attempt


class GenerationError(Exception):
    """One failed attempt."""

    def __init__(
        self,
        message: str,
        *,
        retryable: bool,
        status: int | None = None,
        retry_after: float | None = None,
        body: str = "",
    ) -> None:
        super().__init__(message)
        self.message = message
        self.retryable = retryable
        self.status = status
        self.retry_after = retry_after
        self.body = body


class GenerationFailed(Exception):
    """The sample failed for good (after retries, or at once for a non-retryable error)."""

    def __init__(self, message: str, attempts: int) -> None:
        super().__init__(message)
        self.message = message
        self.attempts = attempts


ProgressCallback = Callable[[Progress, int], None]  # (progress, new tokens since last call)


# --------------------------------------------------------------------------- request building


def _content_text(content: Any) -> str:
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = []
        for part in content:
            if isinstance(part, str):
                parts.append(part)
            elif isinstance(part, dict) and isinstance(part.get("text"), str):
                parts.append(part["text"])
        return "".join(parts)
    return "" if content is None else str(content)


def last_user_text(messages: list[dict[str, Any]]) -> str:
    for m in reversed(messages or []):
        if isinstance(m, dict) and m.get("role") == "user":
            return _content_text(m.get("content"))
    return ""


def prepare_messages(messages: list[dict[str, Any]], params: RunParams) -> list[dict[str, Any]]:
    """The prompt as sent: `system_prompt` replaces any system message, `user_template` wraps the
    last user message. Uses str.replace so braces in prompts are safe."""
    msgs = [dict(m) for m in (messages or []) if isinstance(m, dict)]
    if params.system_prompt is not None and params.system_prompt.strip():
        msgs = [m for m in msgs if m.get("role") != "system"]
        msgs.insert(0, {"role": "system", "content": params.system_prompt})
    template = params.user_template
    if template and "{prompt}" in template:
        for m in reversed(msgs):
            if m.get("role") != "user":
                continue
            content = m.get("content")
            if isinstance(content, str):
                m["content"] = template.replace("{prompt}", content)
            elif isinstance(content, list):
                parts = [dict(p) if isinstance(p, dict) else p for p in content]
                for p in reversed(parts):
                    if isinstance(p, dict) and isinstance(p.get("text"), str):
                        p["text"] = template.replace("{prompt}", p["text"])
                        break
                m["content"] = parts
            break
    return msgs


def deep_merge(base: dict[str, Any], extra: dict[str, Any]) -> dict[str, Any]:
    out = dict(base)
    for key, value in (extra or {}).items():
        if isinstance(value, dict) and isinstance(out.get(key), dict):
            out[key] = deep_merge(out[key], value)
        else:
            out[key] = value
    return out


def build_body(
    model: str,
    messages: list[dict[str, Any]],
    params: RunParams,
    sample_idx: int = 0,
    *,
    include_usage: bool = True,
) -> dict[str, Any]:
    """The JSON body for POST {api_root}/chat/completions. `messages` are already prepared."""
    body: dict[str, Any] = {"model": model, "messages": messages}
    if params.temperature is not None:
        body["temperature"] = params.temperature
    if params.top_p is not None:
        body["top_p"] = params.top_p
    if params.seed is not None:
        body["seed"] = params.seed + sample_idx
    if params.max_tokens is not None:
        body[params.max_tokens_field] = params.max_tokens
    if params.stream:
        body["stream"] = True
        if include_usage:
            body["stream_options"] = {"include_usage": True}
    else:
        body["stream"] = False
    return deep_merge(body, params.extra_body or {})


# --------------------------------------------------------------------------- reasoning split


def _split_harmony(text: str) -> tuple[str | None, str] | None:
    segments = _HARMONY_SEGMENT.findall(text)
    if not segments:
        return None
    thinking: list[str] = []
    final: list[str] = []
    for channel, body in segments:
        name = channel.strip().split(" ")[0] if channel.strip() else ""
        if name == "final":
            final.append(body.strip())
        else:  # analysis, commentary
            thinking.append(body.strip())
    reasoning = "\n\n".join(t for t in thinking if t) or None
    return reasoning, "\n\n".join(final)


def split_inline(text: str, think_open: str = "<think>", think_close: str = "</think>") -> tuple[str | None, str]:
    """Split thinking from the answer when the server returned it inline in `content`.

    Handles `<think>X</think>Y`, `X</think>Y` (the chat template opened the tag), `<think>X` with no
    close (truncated: all thinking, empty answer) and gpt-oss harmony text that leaked through.
    Only whitespace next to a split is stripped; everything else is kept raw.
    """
    if not text:
        return None, text or ""
    if "<|channel|>" in text and "<|message|>" in text:
        harmony = _split_harmony(text)
        if harmony is not None:
            return harmony
    if think_open:
        stripped = text.lstrip()
        if stripped.startswith(think_open):
            rest = stripped[len(think_open) :]
            idx = rest.find(think_close) if think_close else -1
            if idx >= 0:
                reasoning = rest[:idx].strip()
                return (reasoning or None), rest[idx + len(think_close) :].lstrip()
            reasoning = rest.lstrip()
            return (reasoning or None), ""
    if think_close:
        idx = text.find(think_close)
        if idx >= 0:
            before = text[:idx]
            if think_open and think_open in before:
                # Text, then a think block in the middle: not a thinking template. Keep it raw.
                return None, text
            reasoning = before.strip()
            return (reasoning or None), text[idx + len(think_close) :].lstrip()
    return None, text


# --------------------------------------------------------------------------- errors


def _one_line(text: str) -> str:
    return " ".join(str(text).split())


def _sentence(text: str, limit: int = 400) -> str:
    text = _one_line(text)
    if len(text) > limit:
        text = text[: limit - 1].rstrip() + "…"
    if text and text[-1] not in ".!?…":
        text += "."
    return text


def server_message(body: str) -> str | None:
    """The error message inside an error response body, if it has one."""
    body = (body or "").strip()
    if not body:
        return None
    try:
        data = json.loads(body)
    except ValueError:
        if body.startswith("<"):  # an HTML error page from a proxy
            return None
        return _one_line(body)[:300] or None
    msg: Any = None
    if isinstance(data, dict):
        err = data.get("error")
        if isinstance(err, dict):
            msg = err.get("message") or err.get("msg") or err.get("detail") or err.get("type")
        elif isinstance(err, str) and err:
            msg = err
        if not msg:
            detail = data.get("detail")
            if isinstance(detail, str):
                msg = detail
            elif isinstance(detail, list) and detail and isinstance(detail[0], dict):
                msg = detail[0].get("msg")
        if not msg:
            msg = data.get("message") or data.get("msg")
    elif isinstance(data, str):
        msg = data
    if msg is None:
        return None
    if not isinstance(msg, str):
        msg = json.dumps(msg)
    return _one_line(msg)[:300] or None


def describe_status(status: int, body: str, reason_phrase: str = "") -> str:
    """One readable sentence for an HTTP error status, with the server's message when it has one."""
    msg = server_message(body)
    if msg:
        if status == 401:
            return _sentence(f"The server rejected the API key (401): {msg}")
        if status == 403:
            return _sentence(f"The API key is not allowed to do this (403): {msg}")
        return _sentence(f"The server answered {status}: {msg}")
    if status in (401, 403, 404):
        req = httpx.Request("POST", "http://server")
        resp = httpx.Response(status, request=req)
        return describe_http_error(httpx.HTTPStatusError(str(status), request=req, response=resp))
    reason = reason_phrase or ""
    return _sentence(f"The server answered {status} {reason}".strip())


def is_retryable_status(status: int) -> bool:
    return status in RETRY_STATUSES


def is_retryable_exception(exc: BaseException) -> bool:
    if isinstance(exc, GenerationError):
        return exc.retryable
    if isinstance(exc, httpx.HTTPStatusError):
        return is_retryable_status(exc.response.status_code)
    return isinstance(
        exc,
        (
            httpx.TimeoutException,
            httpx.NetworkError,
            httpx.RemoteProtocolError,
            httpx.ProxyError,
            TimeoutError,
        ),
    )


def describe_exception(exc: BaseException) -> str:
    if isinstance(exc, GenerationError):
        return exc.message
    if isinstance(exc, (httpx.ReadError, httpx.RemoteProtocolError, httpx.WriteError, httpx.CloseError)):
        return "The connection to the server dropped before the answer finished."
    if isinstance(exc, httpx.HTTPError):
        return _sentence(describe_http_error(exc))
    return _sentence(str(exc) or exc.__class__.__name__)


def parse_retry_after(headers: httpx.Headers | dict[str, str]) -> float | None:
    """Seconds to wait from Retry-After (seconds or an HTTP date) or retry-after-ms."""
    get = headers.get
    ms = get("retry-after-ms")
    if ms:
        try:
            return max(0.0, float(ms) / 1000.0)
        except ValueError:
            pass
    value = get("retry-after")
    if not value:
        return None
    try:
        return max(0.0, float(value))
    except ValueError:
        pass
    try:
        when = email.utils.parsedate_to_datetime(value)
    except (TypeError, ValueError):
        return None
    if when.tzinfo is None:
        when = when.replace(tzinfo=timezone.utc)
    return max(0.0, (when - datetime.now(timezone.utc)).total_seconds())


def backoff_delay(attempt: int, retry_after: float | None = None) -> float:
    """Seconds to wait before attempt `attempt + 1`: Retry-After when given, else 1, 2, 4… s capped
    at 30 s, with jitter."""
    if retry_after is not None:
        return min(retry_after, MAX_RETRY_AFTER_S)
    base = min(MAX_BACKOFF_S, 2.0 ** max(0, attempt - 1))
    return min(MAX_BACKOFF_S, base * random.uniform(0.75, 1.25))


# --------------------------------------------------------------------------- response parsing


class _Accumulator:
    """Collects one attempt's output from stream chunks or a full message."""

    def __init__(
        self,
        params: RunParams,
        progress: Progress,
        on_progress: ProgressCallback | None,
        t0: float,
    ) -> None:
        self.think_open = params.think_open or ""
        self.think_close = params.think_close or ""
        self.progress = progress
        self.on_progress = on_progress
        self.t0 = t0
        self.reasoning_parts: list[str] = []
        self.details_parts: list[str] = []
        self.content_parts: list[str] = []
        self.saw_reasoning = False
        self.finish_reason: str | None = None
        self.usage: dict[str, Any] | None = None
        self.chunks = 0
        self.ttft_ms: int | None = None
        self.done = False
        # How streamed content is shown live: None (undecided), "think", "harmony" or "answer".
        self._content_mode: str | None = None
        self._content_head = ""

    # ---- live view

    def _emit(self, phase: str, text: str) -> None:
        p = self.progress
        if self.ttft_ms is None:
            self.ttft_ms = int((time.perf_counter() - self.t0) * 1000)
        self.chunks += 1
        p.tokens += 1
        if p.phase != phase:
            p.phase = phase
            p.tail = text[-TAIL_CHARS:]
        else:
            p.tail = (p.tail + text)[-TAIL_CHARS:]
        if self.on_progress is not None:
            self.on_progress(p, 1)

    def _switch_after(self, marker: str, text: str) -> str | None:
        """If `marker` completes in this chunk, return the text after it."""
        window = self.progress.tail[-len(marker) :] + text
        idx = window.find(marker)
        if idx < 0:
            return None
        return window[idx + len(marker) :]

    def _content_chunk(self, text: str) -> None:
        if self.saw_reasoning:
            self._emit("answering", text)
            return
        mode = self._content_mode
        if mode is None:
            self._content_head += text
            head = self._content_head.lstrip()
            if not head:
                mode = None
            elif self.think_open and head.startswith(self.think_open):
                mode = "think"
            elif head.startswith("<|"):
                mode = "harmony"
            elif (self.think_open and self.think_open.startswith(head)) or "<|channel|>".startswith(head):
                mode = None  # could still become a tag
            else:
                mode = "answer"
            self._content_mode = mode
            if mode is None:
                # Count the token but keep the phase until we know what this is.
                self._emit(self.progress.phase if self.progress.phase != "waiting" else "answering", text)
                return
            if mode == "think":
                after = head[len(self.think_open) :]
                self.progress.phase = "thinking"
                self.progress.tail = ""
                self._emit("thinking", after)
                return
            if mode == "harmony":
                self._emit("thinking", text)
                return
            self._emit("answering", text)
            return
        if mode == "think" and self.progress.phase == "thinking":
            after = self._switch_after(self.think_close, text) if self.think_close else None
            if after is not None:
                self.progress.phase = "answering"
                self.progress.tail = ""
                self._emit("answering", after.lstrip())
            else:
                self._emit("thinking", text)
            return
        if mode == "harmony" and self.progress.phase == "thinking":
            after = self._switch_after(_HARMONY_FINAL_MARK, text)
            if after is not None:
                after = after.split("<|message|>", 1)[-1]
                self.progress.phase = "answering"
                self.progress.tail = ""
                self._emit("answering", after)
            else:
                self._emit("thinking", text)
            return
        if mode == "answer" and self.think_close:
            # The chat template may have opened the think tag: everything so far was thinking.
            after = self._switch_after(self.think_close, text)
            if after is not None:
                self.progress.tail = ""
                self._emit("answering", after.lstrip())
                return
        self._emit("answering", text)

    # ---- parsing

    def feed_delta(self, delta: dict[str, Any]) -> None:
        reasoning: str | None = None
        for key in REASONING_FIELDS:
            value = delta.get(key)
            if isinstance(value, str) and value:
                reasoning = value
                break
        if reasoning is not None:
            self.saw_reasoning = True
            self.reasoning_parts.append(reasoning)
            self._emit("thinking", reasoning)
        details = delta.get("reasoning_details")
        if isinstance(details, list):
            detail_text = []
            for item in details:
                if isinstance(item, dict):
                    t = item.get("text")
                    if not isinstance(t, str):
                        t = item.get("summary")
                    if isinstance(t, str) and t:
                        detail_text.append(t)
            if detail_text:
                joined = "".join(detail_text)
                self.details_parts.append(joined)
                if reasoning is None and not self.saw_reasoning:
                    self._emit("thinking", joined)
        content = _content_text(delta.get("content"))
        if content:
            self.content_parts.append(content)
            self._content_chunk(content)

    def feed_event(self, obj: Any) -> None:
        if not isinstance(obj, dict):
            return
        err = obj.get("error")
        if err:
            msg = server_message(json.dumps({"error": err})) or "unknown error"
            code = err.get("code") if isinstance(err, dict) else None
            status = code if isinstance(code, int) else None
            retryable = status is None or is_retryable_status(status)
            raise GenerationError(
                _sentence(f"The server reported an error mid-answer: {msg}"), retryable=retryable, status=status
            )
        usage = obj.get("usage")
        if isinstance(usage, dict) and usage:
            self.usage = usage
        choices = obj.get("choices")
        if not isinstance(choices, list):
            return
        for choice in choices:
            if not isinstance(choice, dict) or choice.get("index", 0) not in (0, None):
                continue
            delta = choice.get("delta")
            if not isinstance(delta, dict):
                delta = choice.get("message") if isinstance(choice.get("message"), dict) else {}
            self.feed_delta(delta)
            if choice.get("finish_reason"):
                self.finish_reason = str(choice["finish_reason"])

    async def read_sse(self, resp: httpx.Response) -> None:
        pending: list[str] = []
        async for line in resp.aiter_lines():
            if not line.strip():
                if pending:
                    data = "\n".join(pending)
                    pending.clear()
                    self._data(data)
                    if self.done:
                        break
                continue
            if line.startswith(":"):
                continue
            if line.startswith("data:"):
                data = line[5:]
                if data.startswith(" "):
                    data = data[1:]
            elif line.lstrip().startswith("{"):
                data = line  # newline-delimited JSON without the SSE prefix
            else:
                continue  # event:, id:, retry:
            if pending:
                pending.append(data)
                continue
            if data.strip() == "[DONE]":
                self.done = True
                break
            try:
                obj = json.loads(data)
            except ValueError:
                pending.append(data)
                continue
            self.feed_event(obj)
        if pending and not self.done:
            self._data("\n".join(pending))

    def _data(self, data: str) -> None:
        if data.strip() == "[DONE]":
            self.done = True
            return
        try:
            obj = json.loads(data)
        except ValueError:
            return
        self.feed_event(obj)

    def read_message(self, data: Any) -> None:
        if not isinstance(data, dict):
            raise GenerationError("The server answered with something that is not a chat completion.", retryable=False)
        if data.get("error"):
            msg = server_message(json.dumps(data)) or "unknown error"
            raise GenerationError(_sentence(f"The server reported an error: {msg}"), retryable=True)
        self.feed_event(data)
        self.done = True

    def result(self, attempts: int, latency_ms: int) -> GenerationResult:
        if self.saw_reasoning:
            reasoning: str | None = "".join(self.reasoning_parts)
        elif self.details_parts:
            reasoning = "".join(self.details_parts)
        else:
            reasoning = None
        response = "".join(self.content_parts)
        if not reasoning:
            reasoning, response = split_inline(response, self.think_open, self.think_close)
        usage = self.usage or {}
        details = usage.get("completion_tokens_details")
        reasoning_tokens = details.get("reasoning_tokens") if isinstance(details, dict) else None
        if reasoning_tokens is None:
            reasoning_tokens = usage.get("reasoning_tokens")
        return GenerationResult(
            reasoning=reasoning or None,
            response=response,
            finish_reason=self.finish_reason,
            prompt_tokens=_int_or_none(usage.get("prompt_tokens")),
            completion_tokens=_int_or_none(usage.get("completion_tokens")),
            reasoning_tokens=_int_or_none(reasoning_tokens),
            latency_ms=latency_ms,
            ttft_ms=self.ttft_ms,
            attempts=attempts,
            chunks=self.chunks,
        )


def _int_or_none(value: Any) -> int | None:
    if isinstance(value, bool) or value is None:
        return None
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


# --------------------------------------------------------------------------- sending


async def _attempt(
    client: httpx.AsyncClient,
    url: str,
    body: dict[str, Any],
    params: RunParams,
    progress: Progress,
    on_progress: ProgressCallback | None,
    attempt: int,
) -> GenerationResult:
    t0 = time.perf_counter()
    acc = _Accumulator(params, progress, on_progress, t0)
    async with client.stream("POST", url, json=body) as resp:
        if resp.status_code >= 400:
            raw = await resp.aread()
            text = raw.decode("utf-8", errors="replace")
            raise GenerationError(
                describe_status(resp.status_code, text, resp.reason_phrase),
                retryable=is_retryable_status(resp.status_code),
                status=resp.status_code,
                retry_after=parse_retry_after(resp.headers),
                body=text[:2000],
            )
        ctype = resp.headers.get("content-type", "").lower()
        if "text/event-stream" in ctype or (body.get("stream") and "json" not in ctype):
            await acc.read_sse(resp)
            if not acc.done and acc.finish_reason is None:
                raise GenerationError(
                    "The stream ended before the server finished the answer.", retryable=True
                )
        else:
            raw = await resp.aread()
            try:
                data = json.loads(raw)
            except ValueError:
                raise GenerationError(
                    "The server answered with something that is not JSON.", retryable=True
                ) from None
            acc.read_message(data)
    latency_ms = int((time.perf_counter() - t0) * 1000)
    return acc.result(attempt, latency_ms)


def _mentions_stream_options(err: GenerationError) -> bool:
    text = (err.body or err.message).lower()
    return "stream_options" in text or "include_usage" in text


async def generate(
    client: httpx.AsyncClient,
    url: str,
    model: str,
    messages: list[dict[str, Any]],
    params: RunParams,
    sample_idx: int = 0,
    *,
    caps: ServerCaps | None = None,
    progress: Progress | None = None,
    on_progress: ProgressCallback | None = None,
    sleep: Callable[[float], Awaitable[Any]] = asyncio.sleep,
) -> GenerationResult:
    """Run one sample to completion. `messages` are the prompt as sent (see `prepare_messages`).

    Retries connect/read errors, timeouts and 408/409/425/429/5xx with backoff, up to
    `params.max_attempts` attempts. Raises GenerationFailed with one readable sentence.
    """
    caps = caps if caps is not None else ServerCaps()
    progress = progress if progress is not None else Progress()
    attempt = 0
    send_stream_options = True
    probe_attempt = 0  # the attempt sent without stream_options after a 400, if any
    while True:
        attempt += 1
        include_usage = send_stream_options and caps.stream_options
        body = build_body(model, messages, params, sample_idx, include_usage=include_usage)
        progress.phase = "waiting"
        progress.tokens = 0
        progress.tail = ""
        progress.attempt = attempt
        try:
            async with asyncio.timeout(params.request_timeout_s):
                result = await _attempt(client, url, body, params, progress, on_progress, attempt)
            if probe_attempt and probe_attempt == attempt:
                caps.stream_options = False  # it worked without: remember for the run
            return result
        except GenerationError as exc:
            error = exc
        except TimeoutError:
            error = GenerationError(
                f"The request did not finish within the run's request timeout of {params.request_timeout_s:g} s.",
                retryable=True,
            )
        except httpx.HTTPError as exc:
            error = GenerationError(describe_exception(exc), retryable=is_retryable_exception(exc))

        if error.status in (400, 422) and "stream_options" in body and not probe_attempt:
            # Some servers reject stream_options. Try once without it (not counted as an attempt).
            send_stream_options = False
            if _mentions_stream_options(error):
                caps.stream_options = False
            attempt -= 1
            probe_attempt = attempt + 1
            continue
        if not error.retryable or attempt >= params.max_attempts:
            raise GenerationFailed(error.message, attempt)
        progress.phase = "waiting"
        progress.tail = ""
        await sleep(backoff_delay(attempt, error.retry_after))
