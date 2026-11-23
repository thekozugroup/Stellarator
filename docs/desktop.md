# Stellarator desktop app

The desktop app is one process: the window and the Stellarator server. The interface is Rust on
nsh 0.6.1 (`app/`), drawn with each platform's own toolkit: AppKit on macOS, Win32 on Windows and
GTK 4 on Linux. The server runs inside the same process and opens no network port: the window
calls its router directly. The browser UI that the server and the Docker image serve is the same
code compiled to WebAssembly. The app is built for a Mac that also runs a local model (LM Studio,
Ollama, MLX), where memory matters.

## Install

Download the file for your computer from the
[latest release](https://github.com/thekozugroup/Stellarator/releases/latest):

| Computer | File | Needs |
| --- | --- | --- |
| Mac with Apple Silicon (M1 or later) | `Stellarator_<version>_aarch64.dmg` | macOS 14 or later |
| Mac with Intel | `Stellarator_<version>_x64.dmg` | macOS 14 or later |
| Windows | `Stellarator_<version>_x64-setup.exe` or `Stellarator_<version>_x64_en-US.msi` | Windows 10 or 11, 64-bit |
| Linux | `Stellarator_<version>_amd64.deb` or `Stellarator_<version>_amd64.AppImage` | GTK 4.14 or later: Ubuntu 24.04+, Debian 13+, Fedora 40+ |

Every build of `.github/workflows/release-nsh.yml` also uploads the installers as workflow
artifacts (`stellarator-macos-aarch64`, `stellarator-macos-x64`, `stellarator-windows-x64`,
`stellarator-linux-amd64`), each a zip with the installers inside.

### macOS

1. Open the `.dmg` and drag Stellarator to Applications.
2. A build without an Apple Developer ID is ad hoc signed and not notarized, and macOS refuses the
   first open ("cannot be opened" or "is damaged"). Remove the download quarantine once:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Stellarator.app
   ```

   Or, without a terminal: open the app once, then click Open Anyway in System Settings > Privacy
   & Security. A notarized build opens directly.

### Windows

Both installers install for your account only, without administrator rights, under
`%LOCALAPPDATA%\Programs\Stellarator`, with a Start menu shortcut. Uninstall from Settings > Apps.
The `-setup.exe` is made with Inno Setup (`nsh package`), the `.msi` with WiX
(`app/packaging/windows/stellarator.wxs`). An unsigned build triggers SmartScreen: click More info,
then Run anyway.

### Linux

```sh
sudo apt install ./Stellarator_<version>_amd64.deb
```

installs the app under `/usr/lib/stellarator` with `stellarator` on the `PATH`, the menu entry, the
icons and the AppStream metainfo, and pulls in GTK 4 and GStreamer. The AppImage does not bundle
GTK: it runs where GTK 4.14 or later is installed, which every desktop with GNOME 46 or later has:

```sh
chmod +x Stellarator_<version>_amd64.AppImage && ./Stellarator_<version>_amd64.AppImage
```

### Coming from 0.3 (the Tauri app)

Versions up to 0.3.1 were a Tauri app with the React UI in the system's web view. Their updater
cannot install 0.4.0: download it by hand once. 0.4.0 uses the same data folder, so your runs,
prompt sets, endpoints and settings are there. The window's size and the theme start from their
defaults.

- Quit 0.3 before opening 0.4. Versions before 0.4 do not take the data folder's lock (see
  [One copy at a time](#one-copy-at-a-time)), so the two could both write the database.
- macOS: dragging 0.4 to Applications replaces 0.3.
- Windows: the new installers do not remove 0.3. Uninstall it from Settings > Apps.
- Linux: the `.deb` installs over 0.3's package (both are named `stellarator`). A 0.3 AppImage is
  a file of its own; delete it.

## Updates

Builds made with the update signing keys (see [Update signing](#update-signing)) check this
repository's latest release a few seconds after they start, then every 6 hours while they run.
They read `nsh-latest.json` from the release and check its signature against the public key built
into the app. When there is a newer version, a dialog names it and the one you have:

- **Install and Restart** downloads the archive for your computer, checks its size and SHA-256
  against the signed manifest, then stops the server the way quitting does (requests in flight
  finish, runs are paused), replaces the app and starts the new version. Resume paused runs from
  their run page.
- **Download** takes its place where the app cannot replace itself (the `.deb`, an AppImage): it
  opens the release page. Update a `.deb` with `sudo apt install ./Stellarator_<version>_amd64.deb`.
- **Later** stops the dialog for that version until the app is started again.

On a Mac, Stellarator > Check for Updates… checks right away and says whether the app is up to date,
or what went wrong. An automatic check that fails (offline, GitHub unreachable) is written to the
log and nothing is shown. The requests go to `github.com` and GitHub's download servers.

A build without the keys, such as one you build yourself, has no updater: it makes no update
requests and has no Check for Updates… item.

## Where data lives

| OS | Data folder |
| --- | --- |
| macOS | `~/Library/Application Support/com.thekozugroup.stellarator` |
| Windows | `%LOCALAPPDATA%\com.thekozugroup.stellarator` |
| Linux | `~/.local/share/com.thekozugroup.stellarator` |

The folder holds the database (`stellarator.db` with its `-wal` and `-shm` files), `uploads/`
(files waiting to be imported), `exports/` (temporary export files) and `stellarator.lock`. To back
it up, quit the app and copy the folder. To start over, quit the app and delete the folder.

Other files:

- The window's place and the app's own settings (the theme, the Rendered/Raw switch) are in
  `storage.json`: in the data folder on macOS and Linux, in
  `%APPDATA%\com.thekozugroup.stellarator` on Windows.
- Exports you download go to your Downloads folder, under the file name the server gives them. An
  existing file is never overwritten: the new one is saved as `name (1).jsonl`. The saved file is
  then shown in Finder (Explorer, the file manager).

Environment variables are read only when the app is started from a terminal (apps opened from
Finder, the Dock or the Start menu do not see your shell's variables):

| Variable | Effect |
| --- | --- |
| `STELLARATOR_DATA_DIR` | Use this data folder instead of the one above. |
| `HF_TOKEN` | Hugging Face token; wins over the one saved in Settings. Saving it in Settings is simpler. |
| `RUST_LOG` | Log detail, for example `debug`. |

From a terminal: `/Applications/Stellarator.app/Contents/MacOS/Stellarator` on a Mac,
`stellarator` after the `.deb`, `%LOCALAPPDATA%\Programs\Stellarator\Stellarator.exe` on Windows.

## One copy at a time

- Opening the app again brings the running window forward instead of starting another copy
  (macOS does this for app bundles; on Windows and Linux it is nsh.json's `singleInstance`).
- Opening a data folder takes a lock on `stellarator.lock` in it (`crates/core/src/datalock.rs`),
  held until the process ends, crash included. A second process on the same folder, such as
  `stellarator-server` started on the app's folder, shows "Stellarator is already open" with the
  folder's path and quits, so two processes never write one database.

## Window, menus and keys

- The window opens where you left it: its size, its position, and maximized or full screen
  (nsh.json `"restoreBounds"`). It opens hidden and shows once it is ready, so on macOS 26 it shows
  with its 26 pt corners from the first frame.
- macOS menu bar: Stellarator (About, Check for Updates… in builds with the updater, Settings… ⌘,,
  Hide, Hide Others, Show All, Quit), Edit (Undo, Redo, Cut, Copy, Paste, Select All), View (Back
  ⌘[, Enter Full Screen), Window (Minimize, Zoom, the tiling items, Close, the window) and Help
  (Stellarator on GitHub).
- Windows and Linux have no menu bar: Ctrl+[ goes back and Ctrl+, opens Settings; text fields have
  the system's editing keys.
- On macOS the window has no separate title bar, as in macOS 26 apps: the window controls sit at
  the top left of the sidebar panel, and the empty unified toolbar the app gives the window
  (`app/src/platform/macos.rs`) makes macOS 26 draw 26 pt corners and center the controls at
  (26, 26) pt. The sidebar's top band and the toolbar's empty areas move the window; a double
  click zooms it. Windows and Linux keep their system title bars.
- Closing the window quits the app (it is a single-window tool).

## Running next to LM Studio, Ollama or MLX

The app runs on your machine, not in a container, so a model server on the same Mac is at
`localhost`. The Add endpoint form has presets with these base URLs:

| Server | Start it | Base URL |
| --- | --- | --- |
| LM Studio | Developer tab > Start server | `http://localhost:1234/v1` |
| Ollama | runs as an app or `ollama serve` | `http://localhost:11434/v1` |
| MLX (`mlx-lm`) | `mlx_lm.server --model <repo> --port 8080` | `http://localhost:8080/v1` (the llama.cpp preset's URL) |
| llama.cpp | `llama-server --port 8080` | `http://localhost:8080/v1` |
| vLLM, SGLang | on another machine | `http://<its address>:8000/v1`, `:30000/v1` |

Seqs (sequences in flight) should match what the server runs in parallel; more requests only wait
in its queue. LM Studio runs one request at a time per loaded model unless parallel requests are
enabled in its server settings. Ollama runs `OLLAMA_NUM_PARALLEL` requests per model (set it before
starting Ollama, for example `launchctl setenv OLLAMA_NUM_PARALLEL 4` and restart the Ollama app).
The presets start at 4.

A model server on another computer on your network: macOS 15 and later ask once whether
Stellarator may find devices on the local network. Allow it; if you declined, turn it on in
System Settings > Privacy & Security > Local Network.

## Memory

One process holds the window and the server: there is no web view, no helper process and no
Python runtime. Measured in CI (`.github/workflows/nsh.yml`, run 45134173056) on the packaged app
showing the Runs screen with sample data, idle, 6 to 15 seconds after it started:

| Platform | How it was read | Figure |
| --- | --- | --- |
| macOS 26 (Intel runner) | `footprint` (what Activity Monitor's Memory column shows) | 16 MiB footprint, 63 MiB resident |
| Windows Server 2025 (`windows-latest`) | the process's private bytes and working set | 21 MiB private, 58 MiB working set |
| Ubuntu 24.04 (Xvfb, GTK's Cairo renderer) | `/proc/<pid>/smaps_rollup` | 33 MiB anonymous (the app's own memory), 143 MiB resident with shared libraries |

During a run the server's share grows with the sequences in flight (the README gives the server's
figures). Runs keep going whether the window is in front, hidden or minimized.

## How it works

- The server runs inside the app (`app/src/backend.rs`): the screens call its axum router in the
  same process, so there is no port, nothing for another program or web page to reach, and nothing
  for the macOS firewall to ask about.
- File pickers (Import prompts > file) are the system's open panels.
- Links to other sites (Hugging Face, OpenRouter's keys page, GitHub) open in your default browser.
- Quitting (closing the window, Cmd+Q, Dock > Quit) stops the server before the process ends:
  in-flight requests finish and runs are paused, waiting at most 15 seconds. Installing an update
  stops it the same way. A run interrupted any other way is paused at the next start.

## Building it yourself

You need Rust (`rustup`) and the nsh CLI 0.6.1:

```sh
curl -fsSL https://nsh.dev/install.sh | sh     # Windows: irm https://nsh.dev/install.ps1 | iex
```

Per platform: on a Mac, Xcode Command Line Tools (`xcode-select --install`); on Windows, the
Visual Studio C++ build tools (and Inno Setup 6 for the `-setup.exe`); on Linux, GTK 4.14 or later
and GStreamer: `sudo apt install libgtk-4-dev clang libgstreamer1.0-dev
libgstreamer-plugins-base1.0-dev` on Ubuntu 24.04.

```sh
git clone https://github.com/thekozugroup/Stellarator && cd Stellarator/app
cargo run                      # a debug build of the app; logs go to the terminal
cargo test --lib               # the app's unit tests

# The installable app. Start nsh's generated build from app/'s lock file first, so the server's
# dependencies stay at the versions the workspace tests.
mkdir -p .nsh/rust && cp Cargo.lock .nsh/rust/Cargo.lock
nsh package --target macos     # .nsh/out/Stellarator.app (--arch arm64, x86_64 or universal)
nsh package --target windows   # .nsh/out/Stellarator-setup.exe and a .zip
nsh package --target linux     # .nsh/out/Stellarator/ and a .tar.gz
nsh package --target web --out .nsh/web   # the browser UI the server serves
```

The `.dmg`, the `.msi`, the `.deb` and the AppImage are made from those outputs by
`.github/workflows/release-nsh.yml` and the scripts in `app/packaging/`. `app/COMPONENTS.md`
describes the code: the router, the components and how to add a screen.

### The icon

The icon is the Stellarator mark in white glass over a two-colour gradient, periwinkle `#5566dc`
at the top to deep indigo `#2c3684` at the bottom, in the manner of a stock macOS 26 app. The mark
is one thick ring with five soft lobes (a stellarator's five-period plasma ring seen from above);
its one source is `app/assets/mark.svg`, which the app's sidebar also draws.
`app/icon-src/make_icon.py` writes every icon from it:

| File | Used for |
| --- | --- |
| `app/icon-src/icon-macos.svg` | macOS before 26: Apple's 824 px rounded square with continuous corners on the 1024 px canvas, with the standard drop shadow |
| `app/icon-src/icon.svg` | Windows, Linux and the web: the same shape, nearly full-bleed |
| `app/icon-src/icon-square.svg` | a full-bleed square, for places that mask the icon themselves |
| `app/icon-src/Stellarator.icon` | macOS 26: an Icon Composer document (the gradient as the icon's fill, `#2c3684` to `#141a40` in the dark appearance; the mark as one white glass layer) |
| `app/icons/icon-macos.png`, `app/icons/icon.png` | the app's icons, 1024 px, which nsh.json names; `nsh package --target web` makes the browser's favicon, touch icon and manifest icons from `icon.png` |
| `app/assets/icon-192.png` | the icon About shows |

To change the icon, edit `make_icon.py` (the colours are `GRADIENT` and `GRADIENT_DARK`; the
mark's size on the tile is `MARK_SIZE`) or `app/assets/mark.svg`, then run it. It renders the PNGs
with resvg (`cargo install resvg`):

```sh
python3 app/icon-src/make_icon.py
```

On macOS 26 the system draws Liquid Glass itself: CI compiles `Stellarator.icon` with Xcode's
`actool` into `Assets.car`, adds it to the app and sets `CFBundleIconName`, so the app follows the
Dock's default, dark, tinted and clear icon styles. If `actool` fails, the build goes on with the
static icon and the log says so. `.github/workflows/icon.yml` compiles the document alone in a few
minutes; to check it on a Mac, open it in Icon Composer (Xcode 26).

## Builds and releases (CI)

`.github/workflows/release-nsh.yml` builds the six installers on every push to `main` that changes
`app/`, `crates/` or the workflow, on `v*` tags and by hand (Run workflow), and uploads them as
artifacts. Both Mac builds run on GitHub's Intel Mac runners, the Apple Silicon one cross-compiled
(`nsh package --arch arm64`). The Windows job installs and uninstalls both installers; the Linux
job installs the `.deb` in a clean `ubuntu:24.04` container and starts it and the AppImage there
(screenshots in the `linux-install-check` artifact). `.github/workflows/nsh.yml` runs the app's
unit tests and starts the packaged app on each platform, with screenshots and memory readings.

Publishing is on when `PUBLISH` near the top of `release-nsh.yml` is `"true"`. Then a push to
`main` whose version has no published release yet makes a GitHub release tagged `v<version>`: the
workflow creates a draft, the builds upload their installers into it, and it is published as the
latest release when all of them succeed. If a build fails, the draft stays hidden: fix the cause
and push (the next run reuses the draft), or use Re-run failed jobs. Once a version is published,
later pushes with the same version only build the artifacts.

To release a new version:

1. Set the same version in `Cargo.toml` (`[workspace.package]`), `app/Cargo.toml` and
   `app/nsh.json`. The workflow's first job fails if they differ.
2. Update the lock files: `cargo update -w`, then `cargo update -w` in `app/`.
3. Add a `## <version>` section to `CHANGELOG.md`. The release notes start with it, under
   "What's new", followed by the download table.
4. Commit and push to `main`.

The same release can also be started by Actions > Release (nsh) > Run workflow with "Publish a
release" checked, or by pushing a matching tag (`git tag v0.4.0 && git push origin v0.4.0`). Both
refuse a version that is already published.

### Signing on macOS

Without signing secrets the Mac app is ad hoc signed (see [Install](#macos)). To sign it with a
Developer ID and notarize it, add these repository secrets:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE` | The Developer ID Application certificate exported as `.p12`, base64-encoded (`base64 -i cert.p12`) |
| `APPLE_CERTIFICATE_PASSWORD` | The `.p12` password |
| `APPLE_SIGNING_IDENTITY` | The certificate's name, for example `Developer ID Application: The Kozu Group (TEAMID1234)` |
| `APPLE_ID` | The Apple ID used for notarization |
| `APPLE_PASSWORD` | An app-specific password for that Apple ID (appleid.apple.com > Sign-In and Security) |
| `APPLE_TEAM_ID` | The 10-character team ID |

Without `APPLE_ID`, `APPLE_PASSWORD` and `APPLE_TEAM_ID` the app is signed but not notarized.
Windows builds are unsigned.

### Update signing

The updater is on only in builds made with both of these repository secrets. The keys are nsh's
(Ed25519); the minisign keys of the 0.3 Tauri app do not fit. Make them on your own Mac, never in
CI, with the nsh CLI:

```sh
mkdir -p ~/.nsh-keys && cd ~/.nsh-keys
nsh keygen stellarator-updates   # writes stellarator-updates.secret and stellarator-updates.pub
gh secret set NSH_UPDATER_SECRET_KEY --repo thekozugroup/Stellarator < stellarator-updates.secret
gh secret set NSH_UPDATER_PUBLIC_KEY --repo thekozugroup/Stellarator < stellarator-updates.pub
```

| Secret | Value |
| --- | --- |
| `NSH_UPDATER_SECRET_KEY` | The secret key: the contents of `stellarator-updates.secret` |
| `NSH_UPDATER_PUBLIC_KEY` | The public key: the contents of `stellarator-updates.pub` |

Keep `stellarator-updates.secret` in a password manager or another backup and never commit it.
Every installed copy trusts only the matching public key: if the secret key is lost, no new update
can be signed for them, and their users download the next version by hand.

With both secrets set, each build adds nsh.json's `"updater"` section (the manifest URL and the
public key; `app/ci/updater-config.py`) and uploads the update archive
(`Stellarator-<version>-<os>-<arch>.tar.gz`, `.zip` on Windows). The publish job writes
`nsh-latest.json` (each platform's archive with its size and SHA-256) and its signature
`latest.json.sig` with `nsh update-manifest`, and uploads both. The manifest is not named
`latest.json`: that name is the 0.3 Tauri app's, whose updater would read it and fail. Without the
secrets the builds have no updater and the release has no manifest.

## The browser UI in the server

`stellarator-server` (Docker, and `cargo run -p stellarator-server`) serves the app's web build
from `app/.nsh/web/Stellarator` (`nsh package --target web --out .nsh/web` in `app/`): release
builds embed it, debug builds read it from disk while they run. A server built without it answers
"The web UI is not built" and its API works as usual. The Content-Security-Policy allows
`'wasm-unsafe-eval'` (compiling the WebAssembly; no JavaScript eval) and `blob:` images; `.wasm` is
served as `application/wasm`, the build's `.br` and `.gz` files go to browsers that accept them,
and unknown paths get `index.html`, so the screens' addresses (`/runs/4`, `/prompts/import`) open
directly. [docker.md](docker.md) covers the image.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| macOS says the app "is damaged" or "cannot be opened" | `xattr -dr com.apple.quarantine /Applications/Stellarator.app` (see [Install](#macos)). |
| A dialog says "Stellarator is already open" | Another process uses the same data folder: the app (another account's copy, or 0.3), or a `stellarator-server` started on it. Quit it, then open Stellarator again. |
| A dialog says "Stellarator could not start" | The reason is in the dialog. For more, start the app from a terminal (see [Where data lives](#where-data-lives)) and read the log. |
| Opening the app shows the existing window | Only one copy runs at a time. |
| Linux: the app does not start, with an error about `libgtk-4.so.1` or a GTK version | It needs GTK 4.14 or later (Ubuntu 24.04, Debian 13, Fedora 40 or later). The `.deb` installs it; the AppImage uses the system's. |
| A local endpoint fails to connect | Check the model server is running: `curl http://localhost:1234/v1/models` (use its port). |
| An endpoint on another computer fails on macOS 15+ | Allow Local Network access for Stellarator in System Settings > Privacy & Security > Local Network. |
| Quitting takes a few seconds | Runs are being paused; it waits at most 15 seconds. |
| Check for Updates… says "Could not check for updates" | The reason is in the dialog: offline, GitHub unreachable, or a latest release without `nsh-latest.json` (made without the update signing secrets). |
| After an update, macOS asks again for Local Network access | Builds that are not signed with a Developer ID get a new signature with each version, and macOS asks again. Allow it once more. |
