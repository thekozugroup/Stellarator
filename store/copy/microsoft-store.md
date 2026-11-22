# Microsoft Store listing

The fields of a Microsoft Store listing (Partner Center, Store listings) for the Windows app. Text
inside a box is what goes into the field; the count next to each limited field is the length of
that text. Screenshots (1920 x 1080) are in `store/images/microsoft-store/`; the 1024 x 1024 icon is
`store/images/icon/stellarator-1024.png`.

## Product name

```text
Stellarator
```

## Short description (246 of 1,000 characters)

Shown at the top of the listing.

```text
Stellarator collects training data from language models. Point it at any OpenAI-compatible server, give it prompts, and it keeps each prompt with the model's thinking trace and answer, then exports datasets for fine-tuning or preference training.
```

## Description (2,476 of 10,000 characters)

```text
Stellarator collects training data from language models. Point it at a model server and a set of prompts. It sends every prompt to the model, keeps the prompt with the model's thinking trace and its answer, and exports the result as a dataset in the format your trainer reads.

It works with any server that offers the OpenAI chat completions API: LM Studio, Ollama or llama.cpp on this PC, vLLM or SGLang on a GPU server, or a hosted API such as OpenRouter. Stellarator does not train or serve models. It talks to the server you already have.

Distillation
Gather prompt, thinking trace and answer from a strong model to fine-tune a smaller one. Stellarator finds the thinking trace wherever the server puts it: in a reasoning_content or reasoning field, or inline in <think> tags.

Preference data
Collect two or more answers per prompt, or run the same prompts against two models. Compare the answers side by side, mark one chosen and one rejected with the mouse or the number keys, and export the pairs for DPO, ORPO or KTO.

Runs you can leave running
Change the number of sequences in flight (seqs) while a run is going; the throughput chart shows what the change did. Failed requests are retried with backoff, and after repeated failures in a row the run pauses itself instead of working through the rest of the prompts. Every failure is counted and can be retried. A run interrupted by quitting is paused, and resumes where it stopped.

Prompts, and only the prompts
Import prompts from Hugging Face datasets by link, from Markdown, text, CSV, TSV, JSON, JSONL or Parquet files, or from pasted text. When you take prompts from someone else's dataset, answer columns such as response, chosen and rejected are left out, and so are assistant turns. The preview shows what was kept and what was left out before anything is saved.

The file you get
Export chat messages, ShareGPT, Alpaca, prompt-completion or raw records for fine-tuning, and DPO or KTO pairs for preference training, as JSONL, JSON, Parquet or CSV, or push them to the Hugging Face Hub. The export dialog shows the first record exactly as it will be written.

Private by design
Your prompts, answers and settings stay on your PC. There is no account and no sign-in. Stellarator sends requests only to the servers you add, and to Hugging Face when you import a dataset or push one there.

Stellarator is free and open source under the MIT license. The source code is at github.com/thekozugroup/Stellarator.
```

## What's new in this version (479 of 1,500 characters)

```text
A new look: Inter for text and Roboto Mono for data, neutral grays with a periwinkle accent, and cards set apart by tone. Detail screens open with a large title, chips for the status, model, endpoint and prompt set, and the key facts as tiles. Menus, dialogs, sheets and the floating action bars use a frosted material. The run chart counts back from now, leaves out paused time, and shows the change in tokens per second after you change seqs. A simpler mark and a new app icon.
```

## Product features (longest 115 of 200 characters each)

Up to 20; one per line.

```text
Works with any OpenAI-compatible server, with presets for OpenRouter, LM Studio, Ollama, vLLM, SGLang and llama.cpp
Lists a server's models and checks that its API key is accepted
Imports Hugging Face datasets by link, any size, downloading only the columns it needs
Imports Markdown, plain text, CSV, TSV, JSON, JSONL and Parquet files, or pasted text
Keeps only the prompts: answer columns and assistant turns from other models are left out
Shows the first prompts, and what was left out, before an import is saved
Change seqs while a run is going, with throughput drawn against seqs
Retries failed requests with backoff and pauses a run after repeated failures
Pause, resume and retry failed samples
Samples per prompt, temperature, top-p, max tokens, seed, system prompt, prompt template and extra request fields
Finds thinking traces in reasoning_content, reasoning or inline <think> tags
Shows each sample's prompt, thinking trace and answer as Markdown with code, tables and math, or raw
Side-by-side preference review with keyboard shortcuts
Exports chat messages, ShareGPT, Alpaca, prompt-completion and raw records for fine-tuning
Exports DPO and KTO pairs for preference training
Puts the thinking trace inline in <think> tags, in a reasoning_content or thinking field, or leaves it out
Writes JSONL, JSON, Parquet or CSV, or pushes to the Hugging Face Hub
Shows the first record of an export exactly as it will be written
Data stays on your PC: no account, no sign-in, no analytics
Light and dark themes that follow Windows
```

## Search terms (longest 18 of 30 characters each)

Up to 7.

```text
LLM distillation
dataset generation
fine-tuning data
preference data
DPO dataset
synthetic data
reasoning traces
```

## Category

- Category: Developer tools
- Subcategory, if asked: Utilities

## System requirements

| | Minimum | Recommended |
| --- | --- | --- |
| OS | Windows 10 (64-bit) | Windows 11 |
| Architecture | x64 | x64 |
| Memory | Not set; the app's own process uses tens of MB, plus the WebView2 runtime | Not set |
| Other | The Microsoft Edge WebView2 runtime (the installer adds it when missing) | |

Additional requirement text:

```text
A model server that this PC can reach: on the PC itself (LM Studio, Ollama, llama.cpp) or on another computer (vLLM, SGLang), or a hosted API key.
```

## Privacy policy URL

`https://digital.kozugroup.com/stellarator/privacy` (placeholder: host `privacy.md` there and use
its real address).

## Website and support

- Website: `https://digital.kozugroup.com/stellarator` (placeholder path)
- Support contact info: `https://github.com/thekozugroup/Stellarator/issues`

## Copyright and trademark info

```text
© 2026 The Kozu Group
```

## Pricing

Free. No in-app purchases, no ads.

## Age rating

Answer the IARC questionnaire as an app (not a game): no violence, sexual content, gambling,
user-to-user communication, location sharing, purchases or ads. The expected rating is the lowest
(3+ / Everyone).

## Before a real submission

The Store takes the `.msi` or `-setup.exe` as an unpackaged Win32 app, but requires it to be signed
with a code-signing certificate; the current builds are unsigned. The installer must install
silently (the NSIS `-setup.exe` supports `/S`). Builds without the update signing key have no
in-app updater, so the Store would be the only source of updates.
