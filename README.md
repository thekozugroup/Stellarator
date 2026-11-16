# Stellarator

Stellarator collects training data from language models. Point it at any OpenAI-compatible
endpoint, pick a model, give it prompts, and it records each prompt with the model's thinking trace
and final response. Export the result as a dataset for supervised fine-tuning (distillation), or
pick a chosen and a rejected response per prompt and export preference pairs for DPO or KTO.

It runs as one Docker container with a web UI.

## Quick start

```bash
docker compose up -d --build
```

Open http://localhost:8080. Data is kept in the `stellarator-data` volume.

To try it without a GPU, start the bundled mock server too:

```bash
docker compose --profile mock up -d --build
```

and add an endpoint with base URL `http://mock:9100/v1`.

Docker details, DGX Spark notes and network options are in [docs/docker.md](docs/docker.md).

## How it works

1. **Add an endpoint.** Any server that speaks the OpenAI chat completions API: vLLM, SGLang,
   llama.cpp, Ollama, LM Studio, TensorRT-LLM, NIM, or a hosted API. A model server on the same
   machine as Docker is at `http://host.docker.internal:<port>/v1`. Stellarator lists the models it
   serves and checks that the API key is accepted (some servers list models without a key).
2. **Import prompts.** From a Hugging Face dataset (by name, with config, split and column
   choice), or a file: Markdown, plain text, CSV, TSV, JSON, JSONL or Parquet. You can also paste
   text. Text and Markdown are split into prompts by separator lines, headings, list items, blank
   lines or lines. Columns that hold chat conversations (OpenAI `messages` or ShareGPT
   `conversations`) are kept as multi-turn prompts. You see the first prompts before importing.
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

Backend (Python 3.12, FastAPI, SQLite):

```bash
cd backend
uv sync
uv run uvicorn stellarator.main:app --reload --port 8000
uv run --dev pytest -q
```

Frontend (Vite, React, Tailwind, shadcn/ui), proxying `/api` to port 8000:

```bash
cd frontend
npm install
npm run dev
```

Mock OpenAI-compatible server (models that return thinking traces in each supported shape):

```bash
uv run --project backend python tools/mock_server.py --port 9100
```

`PRODUCT.md` and `DESIGN.md` record who the tool is for and its visual system.

## Security

There is no sign-in. Run it on a trusted network and do not expose the port to the internet. API
keys for endpoints and the Hugging Face token are stored unencrypted in the SQLite database in the
data volume.
