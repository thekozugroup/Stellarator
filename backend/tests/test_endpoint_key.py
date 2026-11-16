"""The endpoint check catches a rejected API key even when GET /models works without one."""

import httpx
import respx

from stellarator.services.openai_compat import check_key

ROOT = "https://api.example.test/v1"


async def test_rejected_key_is_reported():
    with respx.mock(assert_all_called=False) as router:
        router.post(f"{ROOT}/chat/completions").respond(401, json={"error": {"message": "User not found."}})
        msg = await check_key(ROOT, "sk-bad")
    assert msg == "The server rejected the API key (401): User not found."


async def test_accepted_key_passes_on_bad_request():
    with respx.mock(assert_all_called=False) as router:
        route = router.post(f"{ROOT}/chat/completions").respond(400, json={"error": {"message": "model is required"}})
        assert await check_key(ROOT, "sk-good") is None
    sent = route.calls[0].request
    assert sent.headers["authorization"] == "Bearer sk-good"
    assert b'"max_tokens":1' in sent.content.replace(b" ", b"")


async def test_network_error_is_left_to_the_models_check():
    with respx.mock(assert_all_called=False) as router:
        router.post(f"{ROOT}/chat/completions").mock(side_effect=httpx.ConnectError("down"))
        assert await check_key(ROOT, None) is None


async def test_endpoint_api_marks_rejected_key(client):
    with respx.mock(assert_all_called=False) as router:
        router.get(f"{ROOT}/models").respond(200, json={"data": [{"id": "m1"}]})
        router.post(f"{ROOT}/chat/completions").respond(401, json={"error": {"message": "User not found."}})
        router.route(host="test").pass_through()
        r = await client.post("/api/endpoints", json={"name": "Hosted", "base_url": ROOT, "api_key": "sk-bad"})
        body = r.json()
        assert r.status_code == 201
        assert body["last_ok"] is False and body["models"] == ["m1"]
        assert body["last_error"].startswith("The server rejected the API key (401)")
        t = (await client.post("/api/endpoints/test", json={"base_url": ROOT, "api_key": "sk-bad"})).json()
        assert t["ok"] is False and t["models"] == ["m1"] and "401" in t["error"]
