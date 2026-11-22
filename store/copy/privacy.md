# Stellarator privacy policy

Last updated: November 22, 2026

This policy covers Stellarator, the desktop app for macOS, Windows and Linux and the Docker image,
made by The Kozu Group. It is short because Stellarator collects nothing: The Kozu Group has no
server that Stellarator sends data to.

## What Stellarator stores, and where

Everything stays on the computer that runs Stellarator, in its data folder:

| System | Data folder |
| --- | --- |
| macOS | `~/Library/Application Support/com.thekozugroup.stellarator` |
| Windows | `%LOCALAPPDATA%\com.thekozugroup.stellarator` |
| Linux | `~/.local/share/com.thekozugroup.stellarator` |
| Docker | the `stellarator-data` volume (`/data` in the container) |

The folder holds:

- the database: the endpoints you add (name, address, API key and any extra headers), prompt sets,
  runs and their settings, every sample (prompt, thinking trace, answer, token counts and timings),
  preference pairs and settings, including a Hugging Face token if you save one;
- `uploads/`: files you chose to import, until the import finishes;
- `exports/`: export files while they are being written.

API keys and the Hugging Face token are stored as plain text in the database, readable by anyone
who can read the folder.

The app also keeps its window size and position in its config folder, and the web view keeps the
interface settings (theme, the Rendered/Raw switch) in its own storage. Exports you download are
saved to your Downloads folder. Stellarator keeps no log file.

To delete everything, quit the app and delete the data folder.

## Network requests, and when they happen

Stellarator connects only to the following, and only for the reasons given:

| To | When | What is sent |
| --- | --- | --- |
| The model servers you add (for example LM Studio, Ollama, vLLM or OpenRouter) | When you add, test or refresh an endpoint, and while a run is going | The request for the server's model list; then each prompt with the run's settings (system prompt, sampling parameters, extra request fields). The API key and headers you entered go with each request. |
| Hugging Face (`huggingface.co`, `datasets-server.huggingface.co`) | When you load or import a dataset, and when you push an export to the Hub | The dataset name, split and columns to read; for a push, the exported file. Your Hugging Face token goes with these requests if you saved one. |
| GitHub (`github.com` and its download servers) | Only in builds that include the updater: a few seconds after start, every 6 hours, and when you choose Check for Updates | A request for the latest release's update information; if you choose to install, the download of the update. Version 0.3.0 was built without the updater and makes no such requests. |

Requests identify themselves as `stellarator/<version>`. What those services do with requests is
covered by their own policies: Hugging Face's, GitHub's, and those of any hosted model API you use.

Links to other sites (Hugging Face, OpenRouter, GitHub) open in your default browser.

## What Stellarator does not do

- No analytics, telemetry or crash reports.
- No advertising and no tracking.
- No accounts and no sign-in.
- No data sent to The Kozu Group.

## The local server

The desktop app runs its server on `127.0.0.1`, reachable only from the same computer, and it
answers only the app's own window. The Docker image listens on the network you run it on and has
no sign-in: anyone who can reach its port can use it and the API keys saved in it. Run it on a
trusted network only.

## Children

Stellarator is a developer tool. It is not directed at children and collects no data from anyone.

## Changes

If this policy changes, the new version will be posted on this page with a new date.

## Contact

[OWNER: contact line, for example an email address or a contact page]
