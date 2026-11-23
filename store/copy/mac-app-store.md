# Mac App Store listing

The fields of an App Store Connect listing for the macOS app, in the order App Store Connect asks for
them. Text inside a box is what goes into the field; the count next to each limited field is the
length of that text. Screenshots are in `store/images/mac-app-store/`.

Stellarator is not on the App Store. This listing is ready for the day it is, and its text can be
reused anywhere else.

## App information

### Name (11 of 30 characters)

```text
Stellarator
```

### Subtitle (26 of 30 characters)

```text
Distill datasets from LLMs
```

### Category

- Primary: Developer Tools
- Secondary: Productivity

### Copyright

```text
© 2026 The Kozu Group
```

## Version 0.4.0

### Promotional text (154 of 170 characters)

Promotional text can change without a new version.

```text
Send thousands of prompts to any OpenAI-compatible model, keep each thinking trace and answer, and export datasets for fine-tuning. Free, with no account.
```

### Description (3,187 of 4,000 characters)

```text
Stellarator collects training data from language models. Point it at a model server and a set of prompts. It sends every prompt to the model, keeps the prompt with the model's thinking trace and its answer, and exports the result as a dataset in the format your trainer reads.

It works with any server that offers the OpenAI chat completions API: LM Studio, Ollama or llama.cpp on this Mac, vLLM or SGLang on a GPU server, or a hosted API such as OpenRouter. Stellarator does not train or serve models. It talks to the server you already have.

Runs are made to be left running. You can change the number of sequences in flight (seqs) while a run is going, and the throughput chart shows what the change did. Failed requests are retried with backoff, and after repeated failures in a row the run pauses itself instead of working through the rest of the prompts. Every failure is counted and can be retried.

Prompts come from Hugging Face datasets, from files or from pasted text. When you take prompts from someone else's dataset, only the prompts are kept: answer columns such as response, chosen and rejected are left out, and so are assistant turns.

For preference data, collect two or more answers per prompt, or run the same prompts against two models. Compare the answers side by side and mark one chosen and one rejected, with the mouse or the number keys.

Your prompts, answers and settings stay on your Mac. There is no account and no sign-in. Stellarator sends requests only to the servers you add, and to Hugging Face when you import a dataset or push one there.

FEATURES

• Any OpenAI-compatible server, with presets for OpenRouter, LM Studio, Ollama, vLLM, SGLang and llama.cpp
• Lists a server's models and checks that its API key is accepted
• Hugging Face datasets by link or name, any size; only the columns that are needed are downloaded
• Markdown, plain text, CSV, TSV, JSON, JSONL and Parquet files, or pasted text
• A preview of the first prompts, and of what was left out, before anything is saved
• Seqs that you can change while a run is going, with throughput drawn against them
• Retries with backoff; a run pauses itself after repeated failures
• Pause, resume, and retry failed samples
• Samples per prompt, temperature, top-p, max tokens, seed, system prompt, prompt template and extra request fields
• Thinking traces from reasoning_content, reasoning, inline <think> tags or gpt-oss channels
• Each sample's prompt, thinking trace and answer, rendered as Markdown with code, tables and math, or raw
• Side-by-side preference review with keyboard shortcuts, and pairing by run
• Fine-tuning exports: chat messages, ShareGPT, Alpaca, prompt-completion or raw records
• Preference exports: DPO and KTO pairs
• The thinking trace inline in <think> tags, in a reasoning_content or thinking field, or left out
• JSONL, JSON, Parquet or CSV files, saved to disk or pushed to the Hugging Face Hub
• Light and dark appearance

Requires macOS 14 or later, and a model server that this Mac can reach: on the Mac itself or on another computer.

Stellarator is free and open source under the MIT license. The source code is at github.com/thekozugroup/Stellarator.
```

### Keywords (98 of 100 characters)

Commas without spaces. No word from the app's name, and no names of other companies or apps (Apple
rejects keywords like vLLM, Ollama or Hugging Face as other parties' trademarks).

```text
distillation,dataset,LLM,SFT,DPO,KTO,fine-tuning,preference,reasoning,training data,synthetic data
```

### What's New in This Version (1,012 of 4,000 characters)

From the 0.4.0 section of CHANGELOG.md.

```text
Stellarator is now a native Mac app.

• The window is drawn with AppKit instead of a web page, and the server runs inside the app with no network port. Every screen, dialog and export works as before, with the same design.
• Less memory: one process and no web view. Idle on the Runs screen, the app's footprint is about 16 MB.
• Only one copy runs per data folder: opening the app again brings its window forward.
• The window opens where you left it, with its size and position.
• The menu bar has Settings… (Command-comma) in the app menu and Back (Command-[) in View, and the Window menu lists the window.
• Requires macOS 14 or later.
```

### URLs

| Field | Value |
| --- | --- |
| Support URL | `https://digital.kozugroup.com/stellarator` (placeholder path: use the page's real address once it exists) |
| Marketing URL | `https://digital.kozugroup.com/stellarator` (placeholder path, as above) |
| Privacy Policy URL | `https://digital.kozugroup.com/stellarator/privacy` (placeholder: host `privacy.md` there) |

## Age rating: 4+

Answers to the App Store Connect age rating questionnaire:

| Question | Answer |
| --- | --- |
| Cartoon or fantasy violence | None |
| Realistic violence | None |
| Prolonged graphic or sadistic realistic violence | None |
| Profanity or crude humor | None |
| Mature or suggestive themes | None |
| Horror or fear themes | None |
| Medical or treatment information | None |
| Alcohol, tobacco or drug use or references | None |
| Simulated gambling | None |
| Sexual content or nudity | None |
| Graphic sexual content and nudity | None |
| Contests | No |
| Gambling | No |
| Unrestricted web access | No. Links open in the default browser; the app's window shows only its own pages. |
| User-generated content shared with other users | No. Nothing is shared between users. |
| Messaging or chat between users | No |
| Advertising | No |
| Parental controls, age assurance | No |

The app shows text produced by models that the user connects it to. It contains no model and no
content of its own. If App Review treats model output as unrestricted content, say that the user
chooses and runs the model server, and that the app is a developer tool for collecting training
data.

## App Privacy: Data Not Collected

Answer "No, we do not collect data from this app". The reasons:

- Everything Stellarator stores stays on the device, in the app's data folder: endpoints and their
  API keys, prompt sets, runs, samples, preference pairs and settings. The developer has no server
  that receives any of it.
- The app makes network requests only to:
  - the model servers the user adds (to list their models and to send the prompts);
  - Hugging Face (`huggingface.co`, `datasets-server.huggingface.co`), when the user imports a
    dataset or pushes an export to the Hub;
  - GitHub (`github.com` and its download servers), to check for and download updates, in builds
    that include the updater. An App Store build would not include it: the App Store updates the
    app.
- No analytics, no crash reporting, no advertising, no tracking, no accounts.

Data the user sends to a model server or to Hugging Face goes to services the user chose and signs
in to themselves; Apple's definition of collection covers data sent to the developer or its
partners, not to services the user configures.

## Export compliance

| Question | Answer |
| --- | --- |
| Does your app use encryption? | Yes |
| Proprietary or non-standard encryption algorithms? | No |
| Standard encryption algorithms instead of, or in addition to, the encryption in Apple's operating system? | Yes: HTTPS (TLS) through rustls, a standard TLS library built into the app, for connections to model servers, Hugging Face and GitHub |
| What the encryption is for | Securing network connections only. No encryption of stored data, no VPN, no messaging encryption. |

This is standard encryption in a mass-market app (the export classification usually given for this is
ECCN 5D992.c). App Store Connect may ask for documentation or a self-classification report for
standard algorithms that do not come from Apple's operating system; check its current wording when
submitting. A build that made HTTPS requests only through Apple's `URLSession` would qualify for
`ITSAppUsesNonExemptEncryption = NO` without documents, but Stellarator's server does not.

## App Review notes

```text
Stellarator collects training data from language models. It needs a model server to talk to; it has no account and no sign-in.

To try it with a hosted model:
1. Open Endpoints, click Add endpoint, then OpenRouter.
2. Paste this API key: [OWNER: an OpenRouter API key with a small credit limit]. Click Test connection, then Add endpoint.
3. Open Prompt sets > Import prompts > Paste. Paste a few questions separated by lines with three dashes (---) and click Import prompts.
4. Open Runs > New run. Choose the OpenRouter endpoint, the model [OWNER: model id] and the prompt set, then click Start run.
5. On the run page, change Seqs while the run is going; the chart shows the effect. When the run is done, click Export dataset..., then Download JSONL.

To try it without a key: install LM Studio or Ollama on the same Mac, start its server, and add it with the LM Studio or Ollama preset.

Hugging Face import: on the Hugging Face tab of Import prompts, enter openai/gsm8k. Public datasets need no token.

The app's server runs inside the app's own process and opens no network port: nothing on this Mac or on the network can reach it.
```

## Before a real submission

The current builds are made for direct download. An App Store build would need:

- the App Sandbox, with outgoing network connections and read-write access to files the user
  selects (imports and exports); the app opens no port, so it needs no incoming connections;
- no self-updater (builds without the update signing key already have none);
- signing with an Apple Distribution certificate (the current builds are ad hoc signed and not
  notarized unless the Developer ID secrets are set);
- the data folder inside the sandbox container
  (`~/Library/Containers/com.thekozugroup.stellarator/Data/Library/Application Support/...`),
  which changes the paths in docs/desktop.md.
