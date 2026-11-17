# Product

## What it is

Stellarator collects training data from language models. You point it at any OpenAI-compatible
endpoint (a model served on your own hardware, such as vLLM or SGLang on a DGX Spark, or a hosted
API), pick a model, give it a set of prompts, and it runs every prompt through the model and keeps
the prompt, the model's thinking trace and its final response. The result is exported as a dataset
in the format a trainer expects.

It serves two jobs:

- **Distillation**: gather prompt, thinking trace and response triples from a strong model to
  fine-tune a smaller one (SFT).
- **Preference data**: gather several responses per prompt (several samples from one model, or the
  same prompts run against several models), choose a preferred and a rejected response for each
  prompt, and export pairs for DPO, ORPO or KTO.

## Who uses it

One person or a small team training models, usually technical, often on their own GPU box. They run
long jobs (hundreds to hundreds of thousands of requests) and check on them. They care about
throughput, cost of failure (a run that silently fails overnight), and the exact shape of the
exported file.

## Where and how

- Runs as a desktop app (macOS on Apple Silicon first; also Windows and Linux), often on the same
  Mac as a local model (LM Studio, Ollama, MLX), so its own memory use must stay small. Or in Docker
  on the same machine as the model server (a DGX Spark) or next to it, opened in a browser on the
  local network and sometimes checked from a phone.
- No accounts. It is a single-tenant tool with no sign-in; do not expose it to the internet.
- Used in bursts: set up a run (minutes), then glance at it (seconds) many times over hours.

## The words of the trade

Endpoint, base URL, model, prompt set, run, sample, seqs (concurrent sequences in flight), thinking
trace, response, finish reason, truncated (finish reason `length`), tokens per second, chosen,
rejected, pair.

## What it can and cannot do

- Can: any OpenAI-compatible `/v1/chat/completions` server, with presets for OpenRouter, LM Studio,
  Ollama, vLLM, SGLang and llama.cpp; whole Hugging Face datasets imported as prompts only (other
  models' responses and assistant turns left out); reasoning returned as
  `reasoning_content`, `reasoning`, or inline `<think>` tags; prompts from Hugging Face datasets,
  Markdown, plain text, CSV, TSV, JSON, JSONL, Parquet or pasted text; concurrency changed while a
  run is going; pause, resume, retry failed samples; exports in chat-messages, ShareGPT, Alpaca,
  prompt-completion, raw, DPO and KTO formats as JSONL, JSON, Parquet or CSV.
- Cannot: train models; score responses automatically (yet); manage the model server itself.

## Evidence

No customers, quotes or benchmarks exist. All data shown in screenshots or fixtures is sample data.

## Principles

1. A run must never fail silently. Failures are counted, visible and retryable.
2. The exported file is the product. Show exactly what will be written.
3. Concurrency is the user's call, and it can be changed without stopping the run.
4. Keep the raw text. Never rewrite model output beyond splitting thinking from response.
