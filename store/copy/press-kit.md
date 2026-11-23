# Stellarator press kit

## Boilerplate

**Short (one paragraph)**

> Stellarator is a free, open-source app for collecting training data from language models. It
> sends a set of prompts to any OpenAI-compatible model server, keeps each prompt with the model's
> thinking trace and answer, and exports the result as a dataset for fine-tuning or preference
> training. It runs on macOS, Windows and Linux, or as a Docker container on a GPU server, and keeps
> all data on the user's machine.

**Long (two paragraphs)**

> Stellarator is a free, open-source app for collecting training data from language models, made by
> The Kozu Group. People who fine-tune models use it for two jobs: distillation, where a strong
> model's prompts, thinking traces and answers become fine-tuning data for a smaller one, and
> preference data, where several answers to each prompt are compared and paired as chosen and
> rejected for DPO, ORPO or KTO training.
>
> It works with any server that offers the OpenAI chat completions API, from LM Studio and Ollama on
> a laptop to vLLM and SGLang on a GPU server, and with hosted APIs such as OpenRouter. Runs can be
> left going for hours: the number of concurrent requests can be changed while a run is going,
> failed requests are retried, and the exports match the formats that common trainers read. There
> is no account, and the data stays on the user's machine.

## Facts

| | |
| --- | --- |
| Name | Stellarator (one word, capital S) |
| Developer | The Kozu Group |
| Current version | 0.4.0 [OWNER: release date] |
| First release | 0.2.0, November 17, 2026 |
| Price | Free. No in-app purchases, no ads, no account. |
| License | MIT |
| Platforms | macOS 14 or later (Apple Silicon and Intel); Windows 10 and 11 (x64); Linux x86-64 with GTK 4.14 or later (Ubuntu 24.04 or later, Debian 13 or later); Docker on Linux (x86-64 and ARM64), used in a browser |
| Model servers | Any OpenAI-compatible server; presets for OpenRouter, LM Studio, Ollama, vLLM, SGLang and llama.cpp |
| Export formats | Chat messages, ShareGPT, Alpaca, prompt-completion and raw records; DPO and KTO pairs; as JSONL, JSON, Parquet or CSV, or pushed to the Hugging Face Hub |
| Built with | Rust: the server, and the interface on nsh (Native Shell), drawn with AppKit, Win32 or GTK 4, and compiled to WebAssembly for the browser |
| Website | `https://digital.kozugroup.com/stellarator` (placeholder path) |
| Source and downloads | https://github.com/thekozugroup/Stellarator |
| Press contact | [OWNER: contact line] |

There are no user counts, ratings, reviews or customer names to quote. Do not add any.

## Images

All in `store/images/`. PNG, sRGB. The screenshots show sample data: the bundled mock model server
(model names starting `mock-`), a public Hugging Face dataset and a few prepared answers written for
the screenshots.

### Icon

| File | Pixels | Use |
| --- | --- | --- |
| `icon/stellarator-macos-1024.png` | 1024 x 1024 | The macOS icon (rounded square with its shadow, transparent around it): Mac App Store, press, the website on Apple devices |
| `icon/stellarator-macos-512.png` | 512 x 512 | As above, smaller |
| `icon/stellarator-macos-256.png` | 256 x 256 | As above, smaller |
| `icon/stellarator-1024.png` | 1024 x 1024 | The Windows and Linux icon (nearly full-bleed, transparent corners): Microsoft Store, Linux stores |
| `icon/stellarator-512.png` | 512 x 512 | As above, smaller |
| `icon/stellarator-256.png` | 256 x 256 | As above, smaller; favicon and app directories |
| `icon/stellarator-square-1024.png` | 1024 x 1024 | Full-bleed and opaque, for places that cut their own shape (App Store marketing icon, social avatars) |

Colors: periwinkle `#5566dc` and deep indigo `#2c3684` (the icon's gradient), ink `#0b1515`.
Do not recolor the icon, put it on a tile, or add effects to it.

### Website and social

| File | Pixels | Use |
| --- | --- | --- |
| `website/hero.png` | 2400 x 1350 | Page hero: a run in progress, dark appearance |
| `website/og-image.png` | 1200 x 630 | Link previews (Open Graph, X, LinkedIn, Slack) |
| `website/feature-endpoints.png` | 1600 x 1000 | Feature: any OpenAI-compatible server (Add endpoint with presets) |
| `website/feature-import.png` | 1600 x 1000 | Feature: importing prompts from Hugging Face |
| `website/feature-live-run.png` | 1600 x 1000 | Feature: a run in progress, with the throughput chart |
| `website/feature-thinking-trace.png` | 1600 x 1000 | Feature: one sample with its thinking trace and answer |
| `website/feature-preference-review.png` | 1600 x 1000 | Feature: preference review |
| `website/feature-export.png` | 1600 x 1000 | Feature: the export dialog |

### Store screenshots

| Folder | Pixels | Images | Use |
| --- | --- | --- | --- |
| `mac-app-store/` | 2880 x 1800, with 1440 x 900 copies | 6, with headlines | Mac App Store; also good for press, slides and posts |
| `microsoft-store/` | 1920 x 1080 | 4, with headlines, Windows window frame | Microsoft Store; 16:9 slots |
| `linux/` | 1920 x 1080 | 4, plain windows with a Linux title bar | Flathub and other AppStream stores |

The six Mac App Store images, in order:

| File | Headline | Shows |
| --- | --- | --- |
| `01-live-run.png` | Change seqs without stopping the run | A run in progress, dark appearance |
| `02-endpoints.png` | Any OpenAI-compatible server | Add endpoint with the presets, dark |
| `03-import.png` | Import prompts, and only the prompts | Hugging Face import with columns left out, light |
| `04-new-run.png` | Choose the model, prompts and seqs | The New run form, dark |
| `05-preference-review.png` | Choose and reject, side by side | Preference review, light |
| `06-export.png` | See the exact file before exporting | The export dialog, dark |
