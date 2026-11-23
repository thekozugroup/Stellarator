"""/api/mcp-servers and runs with MCP tools: the mock model `mock-tools` calling the mock's MCP
server at <mock>/mcp. The contract server runs in server mode without STELLARATOR_ALLOW_STDIO_MCP,
so stdio servers are refused."""

from __future__ import annotations

import json
import time

import pytest

from shapes import (
    MCP_IMPORT,
    MCP_SERVER,
    RUN,
    SAMPLE,
    SAMPLE_SUMMARY,
    TOOL_SNAPSHOT,
    ListOf,
    check,
    check_trace,
)
from support import assert_error, assert_no_content, get_run, list_samples, wait_run

STDIO_BLOCKED = "Stdio MCP servers run commands on this machine, so they are off in server mode"


@pytest.fixture
def make_mcp(api, trash):
    def make(**body):
        body.setdefault("name", f"mcp-{time.time_ns()}")
        r = api.post("/api/mcp-servers", json=body)
        assert r.status_code == 201, r.text
        server = r.json()
        check(server, MCP_SERVER)
        trash.mcp_servers.append(server["id"])
        return server

    return make


@pytest.fixture
def mock_mcp(mock_root) -> str:
    return f"{mock_root}/mcp"


def test_create_read_list_and_secret_hints(api, make_mcp, mock_mcp):
    s = make_mcp(transport="http", url=mock_mcp, headers={"Authorization": "Bearer sk-contract-0000abcd", "X-Team": "ml"})
    assert s["transport"] == "http" and s["url"] == mock_mcp
    assert s["headers"] == [{"name": "Authorization", "hint": "…abcd"}, {"name": "X-Team", "hint": "…"}]
    assert s["status"] == "unchecked" and s["tools"] == [] and s["blocked"] is None
    assert s["enabled"] is True and s["run_count"] == 0 and s["command"] is None and s["args"] == [] and s["env"] == []
    r = api.get(f"/api/mcp-servers/{s['id']}")
    assert r.status_code == 200 and r.json() == s
    r = api.get("/api/mcp-servers")
    assert r.status_code == 200
    check(r.json(), ListOf(MCP_SERVER))
    assert "sk-contract" not in r.text


def test_update_keeps_secrets_unless_replaced(api, make_mcp, mock_mcp):
    s = make_mcp(transport="http", url=mock_mcp, headers={"Authorization": "Bearer sk-keep-me-1234"})
    r = api.patch(f"/api/mcp-servers/{s['id']}", json={"headers": {"Authorization": None, "X-New": "value-5678"}, "enabled": False})
    assert r.status_code == 200, r.text
    out = r.json()
    check(out, MCP_SERVER)
    assert out["headers"] == [{"name": "Authorization", "hint": "…1234"}, {"name": "X-New", "hint": "…5678"}]
    assert out["enabled"] is False
    assert out["status"] == "unchecked"
    assert_error(
        api.patch(f"/api/mcp-servers/{s['id']}", json={"headers": {"Missing": None}}), 422, field="headers",
        detail="Missing has no saved value to keep. Enter a value.",
    )
    # The kept key still works: the mock without a key accepts any header, so check the server answers.
    api.patch(f"/api/mcp-servers/{s['id']}", json={"enabled": True})
    r = api.post(f"/api/mcp-servers/{s['id']}/check")
    assert r.status_code == 200 and r.json()["status"] == "ok", r.text


def test_validation_errors(api, make_mcp, mock_mcp):
    assert_error(api.post("/api/mcp-servers", json={"name": "a b", "transport": "http", "url": mock_mcp}), 422, field="name")
    assert_error(api.post("/api/mcp-servers", json={"name": "x", "transport": "http"}), 422, field="url")
    assert_error(api.post("/api/mcp-servers", json={"name": "x", "transport": "http", "url": "ws://x"}), 422, field="url")
    assert_error(api.post("/api/mcp-servers", json={"transport": "http", "url": mock_mcp}), 422, field="name")
    assert_error(api.post("/api/mcp-servers", json={"name": "x", "transport": "sse", "url": mock_mcp}), 422, field="transport")
    s = make_mcp(transport="http", url=mock_mcp)
    assert_error(
        api.post("/api/mcp-servers", json={"name": s["name"].upper(), "transport": "http", "url": mock_mcp}), 422,
        field="name", contains="already exists",
    )
    assert_error(api.get("/api/mcp-servers/999999"), 404, detail="MCP server not found")
    assert_error(api.patch("/api/mcp-servers/999999", json={}), 404)
    assert_error(api.delete("/api/mcp-servers/999999"), 404)
    assert_error(api.post("/api/mcp-servers/999999/check"), 404)


def test_stdio_is_refused_in_server_mode(api):
    r = api.get("/api/settings")
    assert r.json()["stdio_mcp_allowed"] is False
    body = assert_error(api.post("/api/mcp-servers", json={"name": "fs", "transport": "stdio", "command": "npx"}), 403)
    assert body["detail"].startswith(STDIO_BLOCKED) and "STELLARATOR_ALLOW_STDIO_MCP=1" in body["detail"]


def test_check_lists_tools_and_reports_failures(api, make_mcp, mock_mcp):
    s = make_mcp(transport="http", url=mock_mcp)
    r = api.post(f"/api/mcp-servers/{s['id']}/check")
    assert r.status_code == 200, r.text
    out = r.json()
    check(out, MCP_SERVER)
    assert out["status"] == "ok" and out["error"] is None and out["checked_at"]
    assert out["server_info"].startswith("stellarator-mock-tools ")
    assert [t["name"] for t in out["tools"]] == ["lookup", "add", "fail", "slow", "chart", "repeat"]
    dead = make_mcp(transport="http", url="http://127.0.0.1:1/mcp")
    out = api.post(f"/api/mcp-servers/{dead['id']}/check").json()
    assert out["status"] == "error" and out["error"].startswith("Couldn't open an MCP session at http://127.0.0.1:1/mcp")


def test_import_pasted_json(api, trash, mock_mcp):
    name = f"imp{time.time_ns()}"
    paste = {
        "mcpServers": {
            name: {"type": "http", "url": mock_mcp},
            f"{name}-sse": {"type": "sse", "url": mock_mcp},
            f"{name}-local": {"command": "npx", "args": ["-y", "something"]},
            f"{name}-bad": {"type": "websocket"},
        }
    }
    r = api.post("/api/mcp-servers/import", json={"text": json.dumps(paste), "check": True})
    assert r.status_code == 200, r.text
    out = r.json()
    check(out, MCP_IMPORT)
    trash.mcp_servers.extend(s["id"] for s in out["created"])
    assert [(s["name"], s["status"]) for s in out["created"]] == [(name, "ok"), (f"{name}-sse", "ok")]
    errors = {e["entry"]: e["message"] for e in out["errors"]}
    assert errors[f"{name}-local"].startswith(STDIO_BLOCKED)
    assert errors[f"{name}-bad"] == 'The transport "websocket" is not supported; use stdio or http.'
    assert out["notes"][0]["entry"] == f"{name}-sse" and "legacy SSE" in out["notes"][0]["message"]
    # The same paste again: names get a suffix.
    r = api.post("/api/mcp-servers/import", json={"text": json.dumps({"servers": {name: {"type": "http", "url": mock_mcp}}})})
    out = r.json()
    trash.mcp_servers.extend(s["id"] for s in out["created"])
    assert out["created"][0]["name"] == f"{name}-2"
    assert out["notes"] == [{"entry": name, "message": f'Saved as "{name}-2": the name "{name}" is taken.'}]
    assert_error(api.post("/api/mcp-servers/import", json={"text": "not json"}), 422, field="text", contains="not valid JSON")


def test_run_with_tools_traces_and_exports(api, make_endpoint, make_prompt_set, make_run, make_mcp, mock_mcp):
    ep = make_endpoint()
    server = make_mcp(transport="http", url=mock_mcp)
    ps = make_prompt_set(text="Look up tides tool:lookup\nAdd 4 and 5 tool:add\nAlways fails tool:fail", mapping={"split_mode": "line"})
    tools = {"servers": [{"server_id": server["id"], "tools": ["lookup", "add", "fail"]}], "max_rounds": 3, "tool_choice": "auto"}
    run = make_run(ep["id"], ps["id"], "mock-tools", tools=tools)
    check(run, RUN)
    assert run["params"]["tools"] == {**tools, "tool_timeout_s": 60.0, "max_result_chars": 20000}
    check(run["tools"], TOOL_SNAPSHOT)
    assert [d["function"]["name"] for d in run["tools"]["definitions"]] == ["lookup", "add", "fail"]
    assert run["tools"]["servers"][0]["name"] == server["name"]
    done = wait_run(api, run["id"])
    assert done["done"] == 3 and done["failed"] == 0 and done["tool_calls"] == 3
    summaries = list_samples(api, run["id"])
    check(summaries, ListOf(SAMPLE_SUMMARY))
    assert all(s["tool_calls"] == 1 for s in summaries)
    for item in summaries:
        sample = api.get(f"/api/samples/{item['id']}").json()
        check(sample, SAMPLE)
        check_trace(sample["trace"])
        assert [m["role"] for m in sample["trace"]] == ["assistant", "tool", "assistant"]
        assert sample["response"] == sample["trace"][-1]["content"]
    errors = [api.get(f"/api/samples/{i['id']}").json()["trace"][1] for i in summaries]
    assert [e["is_error"] for e in errors] == [False, False, True]
    assert api.get(f"/api/mcp-servers/{server['id']}").json()["run_count"] == 1
    # Delete is refused while a run uses the server only when it is running; this one finished.
    # Exports: messages with tools; Alpaca and prompt-completion refused.
    r = api.get(f"/api/runs/{run['id']}/export", params={"format": "messages", "reasoning": "think_tags"})
    assert r.status_code == 200
    rec = json.loads(r.text.splitlines()[0])
    assert [m["role"] for m in rec["messages"]] == ["user", "assistant", "tool", "assistant"]
    assert rec["messages"][1]["tool_calls"][0]["function"]["arguments"] == {"query": "Look up tides"}
    assert rec["tools"] == run["tools"]["definitions"]
    for fmt in ("alpaca", "prompt_completion"):
        assert_error(api.post(f"/api/runs/{run['id']}/export/preview", json={"format": fmt}), 422, field="format")


def test_run_with_tools_validation(api, make_endpoint, make_prompt_set, make_mcp, mock_mcp):
    ep = make_endpoint()
    ps = make_prompt_set(n=1)
    server = make_mcp(transport="http", url=mock_mcp)
    base = {"endpoint_id": ep["id"], "prompt_set_id": ps["id"], "model": "mock-tools"}

    def post(tools):
        return api.post("/api/runs", json={**base, "params": {"tools": tools}})

    assert_error(post({"servers": []}), 422, field="params.tools.servers")
    assert_error(post({"servers": [{"server_id": server["id"]}], "max_rounds": 0}), 422, field="params.tools.max_rounds")
    assert_error(post({"servers": [{"server_id": server["id"]}], "tool_timeout_s": 0}), 422, field="params.tools.tool_timeout_s")
    assert_error(post({"servers": [{"server_id": server["id"]}], "tool_choice": "none"}), 422, field="params.tools.tool_choice")
    assert_error(post({"servers": [{"server_id": 999999}]}), 422, field="params.tools.servers.0.server_id")
    assert_error(post({"servers": [{"server_id": server["id"], "tools": ["nope"]}]}), 422, field="params.tools.servers.0.tools")


def test_delete_refused_while_a_run_uses_the_server(api, make_endpoint, make_prompt_set, make_run, make_mcp, mock_mcp):
    ep = make_endpoint()
    server = make_mcp(transport="http", url=mock_mcp)
    ps = make_prompt_set(n=8)
    run = make_run(ep["id"], ps["id"], "mock-tools", tools={"servers": [{"server_id": server["id"]}]}, concurrency=1)
    assert_error(api.delete(f"/api/mcp-servers/{server['id']}"), 409, contains="A run is using this MCP server")
    api.post(f"/api/runs/{run['id']}/pause")
    wait_run(api, run["id"], "paused")
    assert_no_content(api.delete(f"/api/mcp-servers/{server['id']}"))
    # The paused run cannot resume without its server; it pauses again and says why.
    r = api.post(f"/api/runs/{run['id']}/resume")
    assert r.status_code == 200
    paused = wait_run(api, run["id"], "paused")
    assert "this run uses was deleted" in paused["status_reason"]
    assert get_run(api, run["id"])["tools"]["servers"][0]["server_id"] == server["id"]
