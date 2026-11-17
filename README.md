# Stellarator

Stellarator collects training data from language models. Point it at any OpenAI-compatible
endpoint, pick a model, give it prompts, and it records each prompt with the model's thinking trace
and final response. Export the result as a dataset for supervised fine-tuning (distillation), or
pick a chosen and a rejected response per prompt and export preference pairs for DPO or KTO.

It runs as a desktop app (macOS, Windows, Linux) or as one Docker container with a web UI. Both are
the same Rust server and the same UI.

## Desktop app (macOS, Apple Silicon first)

Download the `.dmg` from the latest [GitHub Actions desktop build](../../actions/workflows/desktop.yml)
or a release, drag Stellarator to Applications, and open it. The app is not signed with an Apple
Developer ID yet, so the first time: right-click it and choose Open (or run
`xattr -dr com.apple.quarantine /Applications/Stellarator.app`).

The app is one native process: the window plus the Rust server inside it, bound to 127.0.0.1 behind
a per-launch token. It idles at about 15 MB for the server and peaks around 30 MB during a run of
256 sequences in flight, so it sits comfortably next to a model running on the same Mac (LM Studio,
Ollama, MLX). Data lives in `~/Library/Application Support/com.thekozugroup.stellarator`. Details
and building it yourself: [docs/desktop.md](docs/desktop.md).

## Docker

```bash
docker compose up -d --build
```

Open http://localhost:8080. Data is kept in the `stellarator-data` volume. To try it without a GPU,
start the bundled mock server too (`docker compose --profile mock up -d --build`) and add an endpoint
with base URL `http://mock:9100/v1`. DGX Spark notes and network options: [docs/docker.md](docs/docker.md).

## How it works

1. **Add an endpoint.** Start from a preset (OpenRouter, LM Studio, Ollama, vLLM, SGLang,
   llama.cpp server) or any server that speaks the OpenAI chat completions API. Stellarator lists
   the models it serves and checks that the API key is accepted (some servers list models without a
   key). From Docker, a model server on the same machine is at `http://host.docker.internal:<port>/v1`.
2. **Import prompts.** From a Hugging Face dataset (one split or all of them, any size), or a
   file: Markdown, plain text, CSV, TSV, JSON, JSONL or Parquet. You can also paste text. Imports
   run in the background with progress; a million rows take a few seconds.
   - **Prompts only.** Taking prompts from someone else's distilled dataset keeps the prompts and
     leaves their model's answers out: response, completion, generation, reasoning, chosen and
     rejected columns are never used as the prompt, and assistant turns in conversations are
     dropped. Choose the whole conversation up to the last user turn, or the first user turn only.
     Reference answers (answer, solution, label) are kept as metadata for later checking.
   - Text and Markdown are split into prompts by separator lines, headings, list items, blank lines
     or lines. You see the first prompts, and what is left out, before importing.
3. **Start a run.** Choose the endpoint, model and prompt set, how many sequences to keep in flight
   (seqs), how many samples per prompt, and the sampling settings. Seqs can be changed while the run
   is going; the run screen plots throughput against seqs so you can see the effect.
4. **Watch it.** The run screen shows progress, tokens per second, the sequences in flight (each
   marked thinking or answering), and every sample. Failed requests are retried with backoff; a run
   pauses itself after repeated failures in a row instead of burning through the prompt set. Runs
   can be paused, resumed and have their failed samples retried. A run interrupted by a restart is
   paused and resumes where it stopped.
5. **Read the results.** Prompts, thinking traces and responses render as Markdown, with tables,
   code and LaTeX math, or as raw text with one switch.
6. **Export.** Download or push to the Hugging Face Hub.

## Thinking traces

Stellarator reads the thinking trace from wherever the server puts it:

- `reasoning_content` (vLLM before 0.10 and SGLang with a reasoning parser, DeepSeek API)
- `reasoning` (vLLM 0.10 and later, OpenRouter, Ollama)
- inline `<think>…</think>` tags in the content, including the case where the chat template
  opened the tag and the output only contains `</think>` (the tags are configurable per run)
- gpt-oss harmony channels, if they leak into the content

For vLLM or SGLang, start the server with a reasoning parser (for example
`--reasoning-parser qwen3` or `deepseek_r1`) to get the trace as a separate field. For models that
need thinking switched on per request, add `{"chat_template_kwargs": {"enable_thinking": true}}` as
the extra request body in the run's advanced options.

## Export formats

Supervised fine-tuning (one record per sample):

| Format | Shape | Used by |
| --- | --- | --- |
| Chat messages | `{"messages": [...]}` | TRL SFTTrainer, Axolotl, Unsloth, most trainers |
| ShareGPT | `{"conversations": [{"from", "value"}]}` | LLaMA-Factory, Axolotl |
| Alpaca | `{"instruction", "input", "output"}` | LLaMA-Factory, older trainers |
| Prompt-completion | `{"prompt": [...], "completion": [...]}` | TRL |
| Raw | prompt, thinking, response, finish reason, usage, latency, parameters | your own pipeline |

Preference (one record per pair):

| Format | Shape | Used by |
| --- | --- | --- |
| DPO | `{"prompt", "chosen", "rejected"}` | TRL DPO, ORPO, CPO, SimPO |
| KTO | `{"prompt", "completion", "label"}` | TRL KTO |

The thinking trace can go inline as `<think>` tags, in a `reasoning_content` field (Qwen3 and
DeepSeek chat templates), in a `thinking` field (gpt-oss chat template), or be left out. Files are
JSONL, JSON, Parquet or CSV. Truncated responses (finish reason `length`) are skipped by default.

## Preference data

Collect several candidates per prompt: set samples per prompt to 2 or more, or run the same prompt
set against several models. Create a preference set from those runs, then for each prompt mark one
candidate chosen and one rejected (keys 1 to 9 and Shift+1 to 9, J and K to move). To pair runs
wholesale, use "Auto-pair by run": every prompt gets the first run's response as chosen and the
second run's as rejected.

## Using it from a phone

The web app works on an iPhone or Android phone on the same network: a tab bar for the four main
screens, 44 pt touch targets, and it can be added to the home screen. Copy buttons work over plain
http on a LAN address.

## Development

The backend is a Rust workspace (`crates/`): `core` (API contract, database, helpers), `engine`
(runs), `import`, `export`, `server` (routes, the embedded web UI, the binary) and `mock` (a fake
OpenAI-compatible server whose models return thinking traces in each supported shape). The desktop
app is in `desktop/` (Tauri 2).

```bash
cd frontend && npm install && npm run build
```

```bash
cargo run -p stellarator-server
```

The server listens on 0.0.0.0:8080 (`STELLARATOR_BIND`) with data in `./data`
(`STELLARATOR_DATA_DIR`). For frontend work, `npm run dev` in `frontend/` proxies `/api` to port
8080. The mock server: `cargo run -p stellarator-mock -- --port 9100`.

Tests:

```bash
cargo test --workspace
```

```bash
cargo build -p stellarator-server -p stellarator-mock && \
  STELLARATOR_START=rust uv run --project tests/contract pytest tests/contract -m "not v1"
```

`PRODUCT.md` and `DESIGN.md` record who the tool is for and its visual system.

## Security

There is no sign-in. Run it on a trusted network and do not expose the port to the internet. API
keys for endpoints and the Hugging Face token are stored unencrypted in the SQLite database in the
data folder. The desktop app accepts requests only from its own window.
