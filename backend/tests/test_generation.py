"""Unit tests for services/generation.py: request building, reasoning extraction, retries."""

from __future__ import annotations

import asyncio
import json
from datetime import datetime, timedelta, timezone
from email.utils import format_datetime

import httpx
import pytest

from stellarator.schemas import RunParams
from stellarator.services.generation import (
    GenerationFailed,
    Progress,
    ServerCaps,
    backoff_delay,
    build_body,
    describe_status,
    generate,
    is_retryable_exception,
    is_retryable_status,
    parse_retry_after,
    prepare_messages,
    split_inline,
)

URL = "http://server/v1/chat/completions"
MSGS = [{"role": "user", "content": "What is 2 + 2?"}]


# --------------------------------------------------------------------------- helpers


def chunk(delta: dict | None = None, finish: str | None = None) -> dict:
    return {"id": "x", "choices": [{"index": 0, "delta": delta or {}, "finish_reason": finish}]}


def usage(prompt: int = 5, completion: int = 7, reasoning: int | None = None) -> dict:
    u: dict = {"prompt_tokens": prompt, "completion_tokens": completion, "total_tokens": prompt + completion}
    if reasoning is not None:
        u["completion_tokens_details"] = {"reasoning_tokens": reasoning}
    return {"id": "x", "choices": [], "usage": u}


def sse(*events: dict, done: bool = True) -> bytes:
    out = "".join(f"data: {json.dumps(e)}\n\n" for e in events)
    if done:
        out += "data: [DONE]\n\n"
    return out.encode()


def stream_response(*events: dict, done: bool = True, split: int | None = None) -> httpx.Response:
    body = sse(*events, done=done)
    if split:
        async def gen():
            for i in range(0, len(body), split):
                yield body[i : i + split]
                await asyncio.sleep(0)

        return httpx.Response(200, headers={"content-type": "text/event-stream"}, content=gen())
    return httpx.Response(200, headers={"content-type": "text/event-stream"}, content=body)


def message_response(message: dict, finish: str = "stop", u: dict | None = None) -> httpx.Response:
    body = {"id": "x", "choices": [{"index": 0, "message": message, "finish_reason": finish}]}
    if u is not None:
        body["usage"] = u["usage"]
    return httpx.Response(200, json=body)


class Recorder:
    """A MockTransport handler that replays responses in order and records requests."""

    def __init__(self, *responses):
        self.responses = list(responses)
        self.requests: list[dict] = []

    async def __call__(self, request: httpx.Request) -> httpx.Response:
        self.requests.append(json.loads(request.content))
        r = self.responses.pop(0) if len(self.responses) > 1 else self.responses[0]
        if isinstance(r, Exception):
            raise r
        if callable(r):
            return await r(request)
        return r


def client_for(handler) -> httpx.AsyncClient:
    return httpx.AsyncClient(transport=httpx.MockTransport(handler))


class Sleeps:
    def __init__(self):
        self.calls: list[float] = []

    async def __call__(self, s: float) -> None:
        self.calls.append(s)


async def run(handler, params: RunParams | None = None, **kw):
    params = params or RunParams()
    sleeps = kw.pop("sleep", None) or Sleeps()
    async with client_for(handler) as client:
        return await generate(client, URL, "m", MSGS, params, kw.pop("sample_idx", 0), sleep=sleeps, **kw)


# --------------------------------------------------------------------------- split_inline


@pytest.mark.parametrize(
    ("text", "reasoning", "response"),
    [
        ("<think>X</think>Y", "X", "Y"),
        ("<think>\nLet me think.\n</think>\n\nThe answer is 4.", "Let me think.", "The answer is 4."),
        ("  <think>X</think>Y", "X", "Y"),
        ("Let me think.\n</think>\n\nThe answer is 4.", "Let me think.", "The answer is 4."),
        ("<think>\nStill thinking when the tokens ran out", "Still thinking when the tokens ran out", ""),
        ("<think>\n\n</think>\n\nNo thinking.", None, "No thinking."),
        ("Just an answer.", None, "Just an answer."),
        ("", None, ""),
        # Raw text is kept: only whitespace next to the split goes.
        ("<think>a  b</think>\n\nY  \n\n  Z  ", "a  b", "Y  \n\n  Z  "),
        ("Text first <think>x</think> y", None, "Text first <think>x</think> y"),
    ],
)
def test_split_inline(text, reasoning, response):
    assert split_inline(text) == (reasoning, response)


def test_split_inline_custom_tags():
    assert split_inline("<reasoning>X</reasoning> Y", "<reasoning>", "</reasoning>") == ("X", "Y")
    # The default tags are left alone when other tags are configured.
    assert split_inline("<think>X</think>Y", "<reasoning>", "</reasoning>") == (None, "<think>X</think>Y")


def test_split_harmony():
    text = (
        "<|channel|>analysis<|message|>We need to add 2 and 2.<|end|>"
        "<|start|>assistant<|channel|>final<|message|>4<|return|>"
    )
    assert split_inline(text) == ("We need to add 2 and 2.", "4")
    # Truncated in the analysis channel: all thinking.
    assert split_inline("<|channel|>analysis<|message|>We need to") == ("We need to", "")
    # Only a final channel.
    assert split_inline("<|start|>assistant<|channel|>final<|message|>Hi<|end|>") == (None, "Hi")


# --------------------------------------------------------------------------- request building


def test_prepare_messages_system_prompt_and_template():
    msgs = [
        {"role": "system", "content": "old system"},
        {"role": "user", "content": "first"},
        {"role": "assistant", "content": "reply"},
        {"role": "user", "content": "f(x) = {x} and {y}"},
    ]
    params = RunParams(system_prompt="Be brief.", user_template="Solve: {prompt}\nShow {work}.")
    out = prepare_messages(msgs, params)
    assert out[0] == {"role": "system", "content": "Be brief."}
    assert [m["role"] for m in out] == ["system", "user", "assistant", "user"]
    assert out[1]["content"] == "first"  # only the last user message is wrapped
    assert out[3]["content"] == "Solve: f(x) = {x} and {y}\nShow {work}."
    assert msgs[3]["content"] == "f(x) = {x} and {y}"  # input untouched


def test_prepare_messages_defaults_keep_prompt():
    msgs = [{"role": "system", "content": "s"}, {"role": "user", "content": "u"}]
    assert prepare_messages(msgs, RunParams()) == msgs
    assert prepare_messages(msgs, RunParams(system_prompt="  ")) == msgs  # blank means unset


def test_build_body_fields_and_merge():
    params = RunParams(
        temperature=0.6,
        top_p=0.95,
        max_tokens=1000,
        max_tokens_field="max_completion_tokens",
        seed=42,
        extra_body={"chat_template_kwargs": {"enable_thinking": True}, "temperature": 1.0,
                    "stream_options": {"continuous_usage_stats": True}},
    )
    body = build_body("m", MSGS, params, sample_idx=3)
    assert body["model"] == "m"
    assert body["messages"] == MSGS
    assert body["top_p"] == 0.95
    assert body["seed"] == 45
    assert body["max_completion_tokens"] == 1000 and "max_tokens" not in body
    assert body["stream"] is True
    assert body["stream_options"] == {"include_usage": True, "continuous_usage_stats": True}
    assert body["chat_template_kwargs"] == {"enable_thinking": True}
    assert body["temperature"] == 1.0  # extra_body is merged last


def test_build_body_omits_unset():
    params = RunParams(temperature=None, top_p=None, max_tokens=None, seed=None, stream=False)
    body = build_body("m", MSGS, params)
    assert body == {"model": "m", "messages": MSGS, "stream": False}
    assert "stream_options" not in build_body("m", MSGS, RunParams(), include_usage=False)


# --------------------------------------------------------------------------- streaming shapes


@pytest.mark.parametrize("field", ["reasoning_content", "reasoning", "thinking"])
async def test_stream_reasoning_fields(field):
    handler = Recorder(
        stream_response(
            chunk({"role": "assistant", "content": ""}),
            chunk({field: "Two plus "}),
            chunk({field: "two."}),
            chunk({"content": "\n\nThe answer"}),
            chunk({"content": " is 4."}),
            chunk({}, finish="stop"),
            usage(5, 7, 3),
        )
    )
    seen: list[tuple[str, int]] = []
    progress = Progress()
    r = await run(handler, progress=progress, on_progress=lambda p, n: seen.append((p.phase, p.tokens)))
    assert r.reasoning == "Two plus two."
    assert r.response == "\n\nThe answer is 4."  # raw when the server split it
    assert r.finish_reason == "stop"
    assert (r.prompt_tokens, r.completion_tokens, r.reasoning_tokens) == (5, 7, 3)
    assert r.attempts == 1 and r.chunks == 4
    assert r.ttft_ms is not None and r.latency_ms >= r.ttft_ms
    assert [p for p, _ in seen] == ["thinking", "thinking", "answering", "answering"]
    assert progress.tail == "The answer is 4."  # tail of the current phase
    req = handler.requests[0]
    assert req["stream"] is True and req["stream_options"] == {"include_usage": True}


async def test_stream_reasoning_details_only_when_nothing_else():
    details = [{"type": "reasoning.text", "text": "Thinking hard."}]
    r = await run(Recorder(stream_response(
        chunk({"reasoning_details": details}), chunk({"content": "4"}), chunk({}, "stop"))))
    assert r.reasoning == "Thinking hard." and r.response == "4"
    # OpenRouter sends both: reasoning wins, details are not appended twice.
    r = await run(Recorder(stream_response(
        chunk({"reasoning": "Thinking hard.", "reasoning_details": details}),
        chunk({"content": "4"}),
        chunk({}, "stop"),
    )))
    assert r.reasoning == "Thinking hard." and r.response == "4"


async def test_stream_inline_tags_split_across_chunks():
    pieces = ["<th", "ink>\nLet me ", "think.\n</th", "ink>\n\nThe answer", " is 4."]
    handler = Recorder(stream_response(*[chunk({"content": p}) for p in pieces], chunk({}, "length"), split=7))
    phases: list[str] = []
    r = await run(handler, on_progress=lambda p, n: phases.append(p.phase))
    assert r.reasoning == "Let me think."
    assert r.response == "The answer is 4."
    assert r.finish_reason == "length"
    assert "thinking" in phases and phases[-1] == "answering"


async def test_stream_whole_think_block_in_one_chunk():
    phases: list[str] = []
    r = await run(Recorder(stream_response(chunk({"content": "<think>X</think>\n\nY"}), chunk({"content": "Z"}),
                                           chunk({}, "stop"))), on_progress=lambda p, n: phases.append(p.phase))
    assert (r.reasoning, r.response) == ("X", "YZ")
    assert phases == ["answering", "answering"]


async def test_stream_open_think_and_truncated_think():
    r = await run(Recorder(stream_response(
        chunk({"content": "Let me think."}), chunk({"content": "\n</think>\n\n4"}), chunk({}, "stop"))))
    assert (r.reasoning, r.response) == ("Let me think.", "4")
    r = await run(Recorder(stream_response(
        chunk({"content": "<think>\nLet me"}), chunk({"content": " think"}), chunk({}, "length"))))
    assert (r.reasoning, r.response, r.finish_reason) == ("Let me think", "", "length")


async def test_stream_harmony_leak():
    pieces = ["<|channel|>analysis<|message|>Add", " them.<|end|><|start|>assistant",
              "<|channel|>final<|message|>4", "<|return|>"]
    phases: list[str] = []
    r = await run(Recorder(stream_response(*[chunk({"content": p}) for p in pieces], chunk({}, "stop"))),
                  on_progress=lambda p, n: phases.append(p.phase))
    assert (r.reasoning, r.response) == ("Add them.", "4")
    assert phases[0] == "thinking" and phases[-1] == "answering"


async def test_stream_plain_without_usage_or_done():
    r = await run(Recorder(stream_response(chunk({"content": "Hi"}), chunk({}, "stop"), done=False)))
    assert (r.reasoning, r.response, r.completion_tokens) == (None, "Hi", None)


async def test_stream_that_ends_early_is_retried():
    handler = Recorder(
        stream_response(chunk({"content": "Hi"}), done=False),
        stream_response(chunk({"content": "Hello"}), chunk({}, "stop")),
    )
    sleeps = Sleeps()
    r = await run(handler, sleep=sleeps)
    assert r.response == "Hello" and r.attempts == 2 and len(sleeps.calls) == 1


# --------------------------------------------------------------------------- non-streaming shapes


@pytest.mark.parametrize(
    ("message", "reasoning", "response"),
    [
        ({"role": "assistant", "content": "4", "reasoning_content": "R"}, "R", "4"),
        ({"role": "assistant", "content": "4", "reasoning": "R"}, "R", "4"),
        ({"role": "assistant", "content": "4", "thinking": "R"}, "R", "4"),
        ({"role": "assistant", "content": "4", "reasoning_details": [{"type": "reasoning.text", "text": "R"}]}, "R", "4"),
        ({"role": "assistant", "content": "<think>R</think>\n\n4"}, "R", "4"),
        ({"role": "assistant", "content": "R</think>4"}, "R", "4"),
        ({"role": "assistant", "content": "<think>R"}, "R", ""),
        ({"role": "assistant", "content": "4"}, None, "4"),
        ({"role": "assistant", "content": None, "reasoning_content": "R"}, "R", ""),
    ],
)
async def test_non_streaming_shapes(message, reasoning, response):
    handler = Recorder(message_response(message, u=usage(3, 9, 2)))
    r = await run(handler, RunParams(stream=False))
    assert (r.reasoning, r.response) == (reasoning, response)
    assert (r.prompt_tokens, r.completion_tokens, r.reasoning_tokens) == (3, 9, 2)
    assert handler.requests[0]["stream"] is False and "stream_options" not in handler.requests[0]
    assert r.ttft_ms is None and r.latency_ms >= 0


async def test_stream_requested_but_server_answers_json():
    r = await run(Recorder(message_response({"role": "assistant", "content": "4", "reasoning": "R"})))
    assert (r.reasoning, r.response, r.finish_reason) == ("R", "4", "stop")


# --------------------------------------------------------------------------- retries and errors


def test_retry_classification():
    for status in (408, 409, 425, 429, 500, 502, 503, 504):
        assert is_retryable_status(status)
    for status in (400, 401, 403, 404, 413, 422, 501):
        assert not is_retryable_status(status)
    req = httpx.Request("POST", URL)
    assert is_retryable_exception(httpx.ConnectError("x", request=req))
    assert is_retryable_exception(httpx.ReadTimeout("x", request=req))
    assert is_retryable_exception(httpx.ReadError("x", request=req))
    assert is_retryable_exception(httpx.RemoteProtocolError("x", request=req))
    assert is_retryable_exception(TimeoutError())
    assert not is_retryable_exception(httpx.UnsupportedProtocol("x", request=req))
    assert not is_retryable_exception(ValueError("x"))


def test_backoff_and_retry_after():
    for attempt, base in ((1, 1), (2, 2), (3, 4), (4, 8)):
        d = backoff_delay(attempt)
        assert base * 0.75 <= d <= base * 1.25
    assert backoff_delay(20) <= 30
    assert backoff_delay(1, retry_after=7) == 7
    assert parse_retry_after(httpx.Headers({"Retry-After": "3"})) == 3
    assert parse_retry_after(httpx.Headers({"retry-after-ms": "1500"})) == 1.5
    when = datetime.now(timezone.utc) + timedelta(seconds=30)
    assert 25 <= parse_retry_after(httpx.Headers({"Retry-After": format_datetime(when, usegmt=True)})) <= 31
    assert parse_retry_after(httpx.Headers({})) is None


def test_describe_status_sentences():
    assert describe_status(500, '{"error": {"message": "Internal error"}}') == "The server answered 500: Internal error."
    assert describe_status(400, '{"detail": "max_tokens is too large"}') == "The server answered 400: max_tokens is too large."
    assert describe_status(401, "") == "The server rejected the API key (401)."
    assert describe_status(401, '{"error": "bad key"}') == "The server rejected the API key (401): bad key."
    assert describe_status(502, "<html><body>Bad gateway</body></html>", "Bad Gateway") == "The server answered 502 Bad Gateway."
    assert "\n" not in describe_status(500, "line one\nline two")


async def test_retries_then_succeeds():
    handler = Recorder(
        httpx.Response(500, json={"error": {"message": "Internal error"}}),
        httpx.Response(429, json={"error": {"message": "Slow down"}}, headers={"Retry-After": "3"}),
        stream_response(chunk({"content": "ok"}), chunk({}, "stop")),
    )
    sleeps = Sleeps()
    r = await run(handler, sleep=sleeps)
    assert r.response == "ok" and r.attempts == 3
    assert 0.75 <= sleeps.calls[0] <= 1.25 and sleeps.calls[1] == 3


async def test_gives_up_after_max_attempts():
    handler = Recorder(httpx.Response(503, json={"error": {"message": "Overloaded"}}))
    sleeps = Sleeps()
    with pytest.raises(GenerationFailed) as info:
        await run(handler, RunParams(max_attempts=3), sleep=sleeps)
    assert info.value.attempts == 3
    assert info.value.message == "The server answered 503: Overloaded."
    assert len(handler.requests) == 3 and len(sleeps.calls) == 2


async def test_client_errors_fail_at_once():
    handler = Recorder(httpx.Response(404, json={"error": {"message": "The model `m` does not exist."}}))
    with pytest.raises(GenerationFailed) as info:
        await run(handler)
    assert info.value.attempts == 1
    assert info.value.message == "The server answered 404: The model `m` does not exist."


async def test_connect_errors_are_retried():
    handler = Recorder(httpx.ConnectError("refused"))
    with pytest.raises(GenerationFailed) as info:
        await run(handler, RunParams(max_attempts=2))
    assert info.value.attempts == 2
    assert info.value.message.startswith("Could not connect.")


async def test_read_error_mid_stream_is_retried():
    async def broken(_request):
        async def gen():
            yield sse(chunk({"content": "partial"}), done=False)
            raise httpx.ReadError("connection reset")

        return httpx.Response(200, headers={"content-type": "text/event-stream"}, content=gen())

    handler = Recorder(broken, stream_response(chunk({"content": "full"}), chunk({}, "stop")))
    r = await run(handler)
    assert r.response == "full" and r.attempts == 2


async def test_total_timeout_per_attempt():
    async def slow(_request):
        await asyncio.sleep(5)
        return stream_response(chunk({"content": "late"}), chunk({}, "stop"))

    params = RunParams(max_attempts=1)
    params.request_timeout_s = 0.05  # below the API minimum, for the test
    with pytest.raises(GenerationFailed) as info:
        await run(Recorder(slow), params)
    assert "request timeout" in info.value.message


async def test_error_event_mid_stream():
    handler = Recorder(stream_response(chunk({"content": "a"}), {"error": {"message": "boom", "code": 400}}))
    with pytest.raises(GenerationFailed) as info:
        await run(handler)
    assert info.value.message == "The server reported an error mid-answer: boom."


async def test_stream_options_rejected_is_remembered():
    rejection = httpx.Response(400, json={"error": {"message": "Unknown field: stream_options"}})
    ok = stream_response(chunk({"content": "ok"}), chunk({}, "stop"))
    handler = Recorder(rejection, ok)
    caps = ServerCaps()
    r = await run(handler, caps=caps)
    assert r.response == "ok" and r.attempts == 1
    assert "stream_options" in handler.requests[0] and "stream_options" not in handler.requests[1]
    assert caps.stream_options is False
    # The next request of the run goes without it.
    handler2 = Recorder(ok)
    await run(handler2, caps=caps)
    assert "stream_options" not in handler2.requests[0]


async def test_unrelated_400_is_not_blamed_on_stream_options():
    bad = httpx.Response(400, json={"error": {"message": "messages must not be empty"}})
    handler = Recorder(bad)
    caps = ServerCaps()
    with pytest.raises(GenerationFailed) as info:
        await run(handler, caps=caps)
    assert info.value.message == "The server answered 400: messages must not be empty."
    assert len(handler.requests) == 2  # one try without stream_options, then give up
    assert caps.stream_options is True


async def test_seed_per_sample():
    handler = Recorder(stream_response(chunk({"content": "ok"}), chunk({}, "stop")))
    await run(handler, RunParams(seed=100), sample_idx=2)
    assert handler.requests[0]["seed"] == 102
