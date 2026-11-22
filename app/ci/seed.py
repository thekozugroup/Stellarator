"""Adds sample runs to a Stellarator data folder, for CI screenshots of the native app.

The app creates the database (its schema) on its first launch; run this after that launch and
before the next one:

    python3 app/ci/seed.py <data folder>

It adds two endpoints, three prompt sets and five runs in the states the Runs list shows
(completed, paused with failures, one with truncations), dated from minutes to two weeks ago. No
samples: the list reads the counts stored on each run. Only the standard library.
"""

import datetime
import json
import sqlite3
import sys

PARAMS = json.dumps({
    "samples_per_prompt": 1, "temperature": 0.7, "top_p": None, "max_tokens": 8192,
    "max_tokens_field": "max_tokens", "seed": None, "system_prompt": None, "user_template": None,
    "extra_body": {}, "stream": True, "think_open": "<think>", "think_close": "</think>",
    "max_attempts": 4, "request_timeout_s": 1800.0, "pause_after_failures": 10,
})


def main(folder: str) -> None:
    now = datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None)

    def ago(**delta: float) -> str:
        return (now - datetime.timedelta(**delta)).isoformat(sep=" ")

    db = sqlite3.connect(f"{folder}/stellarator.db")
    with db:
        db.executemany(
            "INSERT INTO endpoints (id, name, base_url, location, default_concurrency, headers, models,"
            " created_at, updated_at) VALUES (?, ?, ?, 'local', 8, '{}', '[]', ?, ?)",
            [
                (1, "Local vLLM", "http://127.0.0.1:8000/v1", ago(days=15), ago(days=15)),
                (2, "OpenRouter", "https://openrouter.ai/api/v1", ago(days=15), ago(days=15)),
            ],
        )
        db.executemany(
            "INSERT INTO prompt_sets (id, name, source_kind, source, prompt_count, created_at, status,"
            " rows_read, rows_total) VALUES (?, ?, ?, ?, ?, ?, 'ready', ?, ?)",
            [
                (1, "gsm8k train", "huggingface", '{"dataset":"openai/gsm8k","split":"train"}', 7473, ago(days=14), 7473, 7473),
                (2, "Quick checks", "paste", '{"format":"paste"}', 6, ago(days=14), 6, 6),
                (3, "Code review prompts", "file", '{"format":"jsonl"}', 1200, ago(days=4), 1200, 1200),
            ],
        )
        runs = [
            # id, name, endpoint, model, set, seqs, status, total, done, failed, truncated, created
            (1, "mock-flaky on gsm8k train", 1, "mock-flaky", 1, 4, "completed", 400, 388, 12, 31, ago(days=13)),
            (2, "Preference pairs for code review", 2, "deepseek/deepseek-r1", 3, 16, "completed", 1200, 1196, 4, 0, ago(days=3)),
            (3, "qwen3-32b on Quick checks", 1, "qwen3-32b", 2, 8, "completed", 6, 6, 0, 0, ago(hours=26)),
            (4, "Distill gsm8k with long reasoning traces", 1, "qwen3-32b", 1, 12, "paused", 7473, 4200, 37, 0, ago(hours=3)),
            (5, "qwen3-32b on Code review prompts", 1, "qwen3-32b", 3, 32, "paused", 1200, 0, 0, 0, ago(minutes=4)),
        ]
        names = {1: "Local vLLM", 2: "OpenRouter"}
        db.executemany(
            "INSERT INTO runs (id, name, endpoint_id, endpoint_name, model, prompt_set_id, params, concurrency,"
            " status, total, done, failed, truncated, prompt_tokens, completion_tokens, reasoning_tokens,"
            " active_seconds, created_at, started_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 0, 0, 0, ?, ?)",
            [
                (i, name, ep, names[ep], model, ps, PARAMS, seqs, status, total, done, failed, trunc, created, created)
                for (i, name, ep, model, ps, seqs, status, total, done, failed, trunc, created) in runs
            ],
        )
    print(f"seeded {len(runs)} runs into {folder}/stellarator.db")


if __name__ == "__main__":
    main(sys.argv[1])
