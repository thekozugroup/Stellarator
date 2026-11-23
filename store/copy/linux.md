# Linux listing

Copy for Linux app directories (Flathub, GNOME Software and KDE Discover through AppStream, the Snap
Store, AppImageHub). The AppStream metadata itself is
[`store/linux/com.thekozugroup.stellarator.metainfo.xml`](../linux/com.thekozugroup.stellarator.metainfo.xml);
the screenshots it points to are in `store/images/linux/`.

## Name

```text
Stellarator
```

## Summary (26 of 35 characters)

Flathub asks for 35 characters at most, no final period, and no repeat of the name.

```text
Distill datasets from LLMs
```

## Description

The same text as in the metainfo file, for stores that take plain text:

```text
Stellarator collects training data from language models. Point it at a model server, give it a set of prompts, and it sends every prompt to the model, keeps the prompt with the model's thinking trace and its answer, and exports the result as a dataset for fine-tuning or preference training.

It works with any server that offers the OpenAI chat completions API: vLLM, SGLang, llama.cpp, Ollama or LM Studio on your own hardware, or a hosted API such as OpenRouter. It does not train or serve models; it talks to the server you already have.

- Presets for OpenRouter, LM Studio, Ollama, vLLM, SGLang and llama.cpp
- Prompts from Hugging Face datasets, Markdown, text, CSV, TSV, JSON, JSONL or Parquet files, or pasted text
- Only the prompts are imported: answer columns and assistant turns are left out
- Change the number of sequences in flight while a run is going, and see the effect on throughput
- Retries with backoff, and a run that pauses itself after repeated failures
- Thinking traces from reasoning_content, reasoning or inline think tags
- Side-by-side review to mark chosen and rejected answers
- Exports as chat messages, ShareGPT, Alpaca, prompt-completion, raw, DPO or KTO records, in JSONL, JSON, Parquet or CSV, or pushed to the Hugging Face Hub

Data stays on your computer. There is no account and no sign-in.
```

## Facts for the listing

| Field | Value |
| --- | --- |
| App ID | `com.thekozugroup.stellarator` (the `id` in `app/nsh.json`) |
| Developer | The Kozu Group |
| License | MIT (metadata: CC0-1.0) |
| Category | Development |
| Keywords | LLM, distillation, dataset, fine-tuning, SFT, DPO, preference data, OpenAI-compatible |
| Content rating | OARS 1.1, no content (all "none") |
| Homepage | `https://github.com/thekozugroup/Stellarator` (switch to `https://digital.kozugroup.com/stellarator` once that page is live) |
| Bug tracker | `https://github.com/thekozugroup/Stellarator/issues` |
| Requirements | x86-64; GTK 4.14 or later (Ubuntu 24.04 or later, Debian 13 or later, Fedora 40 or later); a model server it can reach |
| Downloads | `Stellarator_0.4.0_amd64.AppImage`, `Stellarator_0.4.0_amd64.deb` |

## Screenshots

| File | Caption |
| --- | --- |
| `store/images/linux/01-live-run.png` | A run in progress: progress, throughput against seqs, and the sequences in flight |
| `store/images/linux/02-import.png` | Importing prompts from a Hugging Face dataset, with the answer columns left out |
| `store/images/linux/03-preference-review.png` | Preference review: candidate answers side by side, with Choose and Reject |
| `store/images/linux/04-export.png` | Export: pick a format and see the first record as it will be written |

The metainfo file links them at `https://raw.githubusercontent.com/thekozugroup/Stellarator/main/store/images/linux/`,
so they resolve once `store/` is on `main`.

## Checking the metainfo file

```bash
appstreamcli validate --pedantic store/linux/com.thekozugroup.stellarator.metainfo.xml
```

It passes with `--no-net`; without that flag it also fetches the screenshot URLs. Flathub runs its
own linter as well (`flatpak run --command=flatpak-builder-lint org.flatpak.Builder appstream ...`).

## Before a Flathub submission

- **The app ID.** Flathub verifies that the developer controls the domain an app ID is built from.
  `com.thekozugroup` stands for `thekozugroup.com`. If The Kozu Group does not own that domain, the
  Flathub build needs an ID it can verify, such as `com.kozugroup.Stellarator` (for
  `kozugroup.com`) or `io.github.thekozugroup.Stellarator`. Changing the ID changes the data folder
  (`~/.local/share/<id>`), so decide before the first Flathub release.
- **The desktop file.** The `.deb` installs `/usr/share/applications/Stellarator.desktop` with
  `Icon=stellarator`. A Flatpak must name both after the app ID; the metainfo file's `launchable`
  already expects `com.thekozugroup.stellarator.desktop`:

  ```ini
  [Desktop Entry]
  Type=Application
  Name=Stellarator
  Comment=Collect thinking traces, responses and preference pairs from language models.
  Exec=stellarator
  Icon=com.thekozugroup.stellarator
  Categories=Development;
  Keywords=LLM;distillation;dataset;fine-tuning;SFT;DPO;
  StartupWMClass=stellarator
  Terminal=false
  ```

- **Install paths.** The metainfo file goes to `/app/share/metainfo/`, the icon to
  `/app/share/icons/hicolor/<size>/apps/com.thekozugroup.stellarator.png` (128, 256 and 512 from
  `store/images/icon/`), and the app needs `--share=network`, `--socket=wayland`,
  `--socket=fallback-x11` and `--device=dri` in the manifest.
