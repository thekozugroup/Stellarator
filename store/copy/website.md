# Website copy: digital.kozugroup.com/stellarator

Copy blocks for the Stellarator page on the owner's site, top to bottom, with the image that goes
with each. Images are in `store/images/website/`. The page address
`https://digital.kozugroup.com/stellarator` is a placeholder path.

## Head

| Tag | Content |
| --- | --- |
| `<title>` | Stellarator: training data from any OpenAI-compatible model |
| `meta name="description"` | Run prompts through any OpenAI-compatible model, keep each thinking trace and answer, and export SFT and preference datasets. Free for Mac, Windows and Linux. |
| `og:title` | Stellarator |
| `og:description` | Distillation and preference data from any OpenAI-compatible model. Free for macOS, Windows and Linux. |
| `og:image` | `og-image.png` (1200 x 630), with `og:image:width` 1200 and `og:image:height` 630 |
| `og:image:alt` | The Stellarator icon and name beside the app window showing a run in progress |
| `twitter:card` | `summary_large_image` |
| Favicon | `store/images/icon/stellarator-256.png`, or `app/assets/web/favicon.svg` |

The title is 59 characters and the description 158, inside the 60 and 160 that search results show.

## Hero

**Headline**

> Distillation and preference data from any OpenAI-compatible model

**Subhead**

> Stellarator runs your prompts through a model, keeps each prompt with its thinking trace and
> answer, and exports the result as a training dataset. A free desktop app for macOS, Windows and
> Linux, or one Docker container next to your GPU.

**Buttons**: "Download for Mac" (primary; show "Download for Windows" or "Download for Linux" to
those systems, linking to the matching section below) and "Source on GitHub" (secondary, to
`https://github.com/thekozugroup/Stellarator`).

**Under the buttons**

> Version 0.4.0 · Free and open source (MIT) · No account

**Image**: `hero.png` (2400 x 1350). Alt text: "Stellarator showing a run in progress: 6,139 of
7,473 samples done, a throughput chart where tokens per second rise after seqs change from 12 to
32, and the sequences in flight."

## Download

> **Download Stellarator 0.4.0**
>
> Free for macOS, Windows and Linux. Older versions and release notes are on the
> [releases page](https://github.com/thekozugroup/Stellarator/releases).

| Computer | File | Size | Link |
| --- | --- | ---: | --- |
| Mac with Apple Silicon (M1 or later), macOS 14 or later | `Stellarator_0.4.0_aarch64.dmg` | 32.1 MB | https://github.com/thekozugroup/Stellarator/releases/download/v0.4.0/Stellarator_0.4.0_aarch64.dmg |
| Mac with an Intel processor, macOS 14 or later | `Stellarator_0.4.0_x64.dmg` | 34.7 MB | https://github.com/thekozugroup/Stellarator/releases/download/v0.4.0/Stellarator_0.4.0_x64.dmg |
| Windows 10 or 11 (installer for your account, no administrator rights) | `Stellarator_0.4.0_x64-setup.exe` | 19.5 MB | https://github.com/thekozugroup/Stellarator/releases/download/v0.4.0/Stellarator_0.4.0_x64-setup.exe |
| Windows 10 or 11 (MSI, for your account, no administrator rights) | `Stellarator_0.4.0_x64_en-US.msi` | 20.1 MB | https://github.com/thekozugroup/Stellarator/releases/download/v0.4.0/Stellarator_0.4.0_x64_en-US.msi |
| Linux (x86-64) with GTK 4.14 or later: Ubuntu 24.04 or later, Debian 13 or later | `Stellarator_0.4.0_amd64.AppImage` | 28.0 MB | https://github.com/thekozugroup/Stellarator/releases/download/v0.4.0/Stellarator_0.4.0_amd64.AppImage |
| Linux (x86-64), as a package: Ubuntu 24.04 or later, Debian 13 or later | `Stellarator_0.4.0_amd64.deb` | 22.2 MB | https://github.com/thekozugroup/Stellarator/releases/download/v0.4.0/Stellarator_0.4.0_amd64.deb |

Each new release changes the version in the six file names, so update the links with it. The sizes
are those of the CI build of the 0.4.0 code (Release (nsh) run 45134172926); check them against
the release's files.

### On a Mac: the first open

> Open the `.dmg` and drag Stellarator to Applications. The app is not notarized yet, so macOS
> blocks the first open. Run this once in Terminal:
>
> ```bash
> xattr -dr com.apple.quarantine /Applications/Stellarator.app
> ```
>
> Or open the app once, then click Open Anyway in System Settings > Privacy & Security.

### On Windows

> Run the installer. It installs for your account, without administrator rights. Windows
> SmartScreen warns about an unsigned app: click More info, then Run anyway.

### On Linux

> ```bash
> sudo apt install ./Stellarator_0.4.0_amd64.deb
> # or
> chmod +x Stellarator_0.4.0_amd64.AppImage && ./Stellarator_0.4.0_amd64.AppImage
> ```

### On a GPU server, in Docker

> Run it next to the model, on a DGX Spark or any Linux server with Docker:
>
> ```bash
> git clone https://github.com/thekozugroup/Stellarator.git
> cd Stellarator
> docker compose up -d --build
> ```
>
> Then open `http://<server>:8080` in a browser on your network, phones included. There is no
> sign-in: keep it on a trusted network.

## Features

Six sections, each a heading, a short paragraph and an image (1600 x 1000). Alternate the image
left and right.

### Any OpenAI-compatible server

Anything that serves `/v1/chat/completions` works: LM Studio, Ollama or llama.cpp on your own
computer, vLLM or SGLang on a GPU server, or a hosted API such as OpenRouter. Presets fill in the
address and a starting number of seqs, and Stellarator lists the server's models and checks the
API key.

Image: `feature-endpoints.png`. Alt text: "The Add endpoint dialog with presets for OpenRouter, LM
Studio, Ollama, vLLM, SGLang and llama.cpp server."

### Prompts, and only the prompts

Paste a Hugging Face link, or import Markdown, text, CSV, TSV, JSON, JSONL or Parquet files, or
pasted text. Answer columns such as `response`, `chosen` and `rejected` are never used as prompts,
and assistant turns are dropped. The preview shows what was kept and what was left out before
anything is saved.

Image: `feature-import.png`. Alt text: "Importing a Hugging Face dataset: the prompt column is
detected, the chosen, rejected and messages columns are left out, and the first prompts are
previewed."

### Runs you can leave running

Change seqs, the number of sequences in flight, while a run is going: the chart draws tokens per
second against seqs, so you see what each change did. Failed requests are retried with backoff,
and a run that keeps failing pauses itself instead of working through the rest of your prompts.

Image: `feature-live-run.png`. Alt text: "A run in progress with its progress, time left,
throughput chart and the sequences in flight."

### Thinking traces, kept apart

Stellarator finds the thinking trace in `reasoning_content`, `reasoning` or inline `<think>` tags
and keeps it apart from the answer. Each sample shows the prompt, the trace and the answer rendered
as Markdown with tables, code and math, or raw, exactly as the model returned it.

Image: `feature-thinking-trace.png`. Alt text: "One sample in its side panel: the prompt, the
thinking trace and the answer with a table."

### Preference pairs, side by side

Collect two or more answers per prompt, or run the same prompts against two models. Mark one
answer chosen and one rejected with the mouse or the number keys, or pair two whole runs at once
with Auto-pair by run.

Image: `feature-preference-review.png`. Alt text: "Preference review: four candidate answers to
the same prompt, each with Choose and Reject."

### The file your trainer reads

Export chat messages, ShareGPT, Alpaca, prompt-completion or raw records, or DPO and KTO pairs, as
JSONL, JSON, Parquet or CSV, or push them to the Hugging Face Hub. The thinking trace goes inline,
in its own field, or is left out, and the first record is shown exactly as it will be written.

Image: `feature-export.png`. Alt text: "The export dialog: formats and thinking-trace options on
the left, the first record as it will be written on the right."

## Privacy band

A one-line band between the features and the FAQ:

> No account, no analytics. Your prompts and answers stay on your computer; Stellarator talks only
> to the servers you add, and to Hugging Face when you import or push a dataset.
> [Privacy policy](https://digital.kozugroup.com/stellarator/privacy)

## FAQ

**Which model servers work?**
Any server that offers the OpenAI chat completions API at `/v1/chat/completions`. There are presets
for OpenRouter, LM Studio, Ollama, vLLM, SGLang and the llama.cpp server; for anything else, such
as MLX's `mlx_lm.server` or another hosted API, enter its base URL.

**Where does my data live?**
On your computer, in the app's data folder: `~/Library/Application Support/com.thekozugroup.stellarator`
on a Mac, `%LOCALAPPDATA%\com.thekozugroup.stellarator` on Windows and
`~/.local/share/com.thekozugroup.stellarator` on Linux. Prompts go only to the model servers you
add, and datasets to Hugging Face only when you import or push one. API keys are kept in that folder
as plain text, so treat it like any folder that holds keys.

**What does it cost?**
Nothing. Stellarator is free and open source under the MIT license. A hosted model API bills you
for the tokens your runs use; a server on your own hardware costs nothing extra.

**Can I run it on the GPU server instead of my laptop?**
Yes. It runs as one Docker container next to the model server (a DGX Spark, for example), and you
open it in a browser on your network, phones included. There is no sign-in, so keep it on a
trusted network. The [Docker guide](https://github.com/thekozugroup/Stellarator/blob/main/docs/docker.md)
has the details.

**Which Mac download do I need?**
Choose Apple menu > About This Mac. If it lists a Chip (Apple M1 or later), download the
`aarch64` file. If it lists an Intel processor, download the `x64` file.

**macOS says the app "is damaged" or "cannot be opened". What do I do?**
The app is not notarized by Apple yet, so macOS blocks the first open. Run
`xattr -dr com.apple.quarantine /Applications/Stellarator.app` once in Terminal, or open the app
once and click Open Anyway in System Settings > Privacy & Security. On Windows, SmartScreen asks
the same: click More info, then Run anyway.

**Does it update itself?**
Builds made with the update signing key check GitHub for new versions and install them; the Linux
`.deb` and AppImage open the download page instead. Version 0.3 and earlier cannot update to 0.4:
download it once from this page. Your data stays in its folder when you install the new version.

**Does Stellarator train or serve models?**
No. It sends prompts to a model server you already run or use, and writes the results as a
dataset for your trainer (TRL, Axolotl, Unsloth, LLaMA-Factory and others).

## System requirements

| | Requirement |
| --- | --- |
| macOS | macOS 14 Sonoma or later, Apple Silicon or Intel |
| Windows | Windows 10 or 11, 64-bit |
| Linux | x86-64 with GTK 4.14 or later: Ubuntu 24.04 or later, Debian 13 or later, Fedora 40 or later |
| Docker | Docker with Compose on Linux, x86-64 or ARM64 (the image builds on the server itself) |
| Model server | Any OpenAI-compatible server the computer can reach, or a hosted API key |
| Memory | One process for the window and the server. Idle on the Runs screen: a 16 MiB footprint on macOS 26 and 21 MiB of private memory on Windows (measured in CI). The server part peaks at about 32 MB during a run with 256 sequences in flight (measured on Linux) |

## Footer

> Stellarator is made by The Kozu Group. Free and open source under the MIT license.
> [GitHub](https://github.com/thekozugroup/Stellarator) · [Changelog](https://github.com/thekozugroup/Stellarator/blob/main/CHANGELOG.md) · [Report an issue](https://github.com/thekozugroup/Stellarator/issues) · [Privacy](https://digital.kozugroup.com/stellarator/privacy)
>
> © 2026 The Kozu Group

## Short descriptions

For directories, link previews and social posts.

**One sentence**

> Stellarator is a free desktop app that runs your prompts through any OpenAI-compatible model and
> exports the prompts, thinking traces and answers as training datasets.

**Three sentences**

> Stellarator is a free, open-source app for collecting training data from language models, for
> macOS, Windows and Linux. Point it at any OpenAI-compatible server (LM Studio, Ollama, vLLM,
> SGLang, llama.cpp or a hosted API), import prompts from Hugging Face or your own files, and it
> keeps each prompt with the model's thinking trace and answer. Export the results as SFT or
> preference datasets, or push them to the Hugging Face Hub; your data stays on your computer.
