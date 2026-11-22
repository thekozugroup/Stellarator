# Stellarator desktop app

The desktop app is one native process: a window (Tauri 2, using the system's webview) and the
Stellarator server, running inside the same process on `127.0.0.1`. It serves the same web UI and
API as the Docker image. It is built for a Mac that also runs a local model (LM Studio, Ollama,
MLX), where memory matters; Windows and Linux builds come from the same code.

## Install

### macOS (Apple Silicon or Intel, macOS 12 or later)

1. Download the disk image:
   - **[Releases](https://github.com/thekozugroup/Stellarator/releases/latest)**: the `.dmg` for
     your Mac (`aarch64` for Apple Silicon, `x64` for Intel).
   - **Actions**: open the latest successful *Desktop* workflow run and download the artifact
     `stellarator-aarch64-apple-darwin` (Apple Silicon) or `stellarator-x86_64-apple-darwin`
     (Intel). The artifact is a zip with the `.dmg` inside.
2. Open the `.dmg` and drag Stellarator to Applications.
3. Builds without an Apple Developer ID certificate are not notarized, and macOS refuses the first
   open ("cannot be opened" or "is damaged"). Remove the download quarantine once:

   ```sh
   xattr -dr com.apple.quarantine /Applications/Stellarator.app
   ```

   Or, without a terminal: on macOS 14 and earlier, Control-click the app, choose Open, then Open
   again. On macOS 15 and later, open it once, then go to System Settings > Privacy & Security and
   click Open Anyway.

### Windows

Run the `.msi`, or the `-setup.exe` (installs for the current user, no administrator rights). An
unsigned build triggers SmartScreen: click More info, then Run anyway. The installer adds the
WebView2 runtime if Windows does not already have it.

### Linux

Ubuntu 22.04 or later, Debian 12 or later (WebKitGTK 4.1):

```sh
sudo apt install ./Stellarator_0.2.0_amd64.deb
# or
chmod +x Stellarator_0.2.0_amd64.AppImage && ./Stellarator_0.2.0_amd64.AppImage
```

## Updates

The app checks this repository's
[latest release](https://github.com/thekozugroup/Stellarator/releases/latest) for a newer version
a few seconds after it starts, then every 6 hours while it runs. When there is one, a dialog names
the new version and the one you have, with two buttons: **Install and Restart** and **Later**.
Later stops the dialog for that version until the app is started again. On a Mac,
Stellarator > Check for Updates… checks right away and says whether the app is up to date, or what
went wrong.

Install and Restart downloads the update (the Dock icon or the taskbar button shows the progress),
then stops the server the way quitting does: requests in flight finish and runs are paused. Then it
installs the update and starts the new version. Resume paused runs from their run page.

| Install | How it updates |
| --- | --- |
| macOS (`.dmg`) | Replaces Stellarator.app with the release's signed `Stellarator_<arch>.app.tar.gz`. If your account cannot write to the app's folder, macOS asks for an administrator password. |
| Windows (`-setup.exe` or `.msi`) | Downloads the same kind of installer and runs it with a progress bar only; the installer opens the new version. The `.msi` asks for administrator rights, as it did when it was installed. |
| Linux AppImage | Replaces the `.AppImage` file. |
| Linux `.deb` | Does not update itself. Install the new `.deb` through the package manager: `sudo apt install ./Stellarator_<version>_amd64.deb`. |

- An automatic check that fails (offline, GitHub unreachable) is written to the log and nothing is
  shown; the next check runs 6 hours later. A failed download or install is shown in a dialog. After
  a failed install the app restarts with the version it had.
- Updates are signed. The app installs a file only when its signature matches the public key built
  into the app and the version in the signature matches the release's `latest.json`.
- The update is not downloaded by a browser, so macOS does not quarantine it: the `xattr` step from
  Install is needed only for the first install.
- The requests go to `github.com` and GitHub's download servers, through the system proxy settings.
- Only release builds made with the update signing key have the updater (see
  [Update signing](#update-signing)). A build without it, such as one you build yourself, makes no
  update requests and has no Check for Updates… item. The first release with the updater has to be
  installed by hand once.

## Where data lives

| OS | Folder |
| --- | --- |
| macOS | `~/Library/Application Support/com.thekozugroup.stellarator` |
| Windows | `%LOCALAPPDATA%\com.thekozugroup.stellarator` |
| Linux | `~/.local/share/com.thekozugroup.stellarator` |

The folder holds the database (`stellarator.db` with its `-wal` and `-shm` files), `uploads/`
(files waiting to be imported) and `exports/` (temporary export files). To back it up, quit the
app and copy the folder. To start over, quit the app and delete the folder.

Other files:

- Exports you download from the app go to your Downloads folder, under the file name the server
  gives them. An existing file is never overwritten: the new one is saved as `name (1).jsonl`.
  The saved file is then shown in Finder (Explorer, the file manager).
- Window size and position: `.window-state.json` in the app's config folder (on macOS the same
  folder as the data).
- The web UI's own settings (theme, the Rendered/Raw switch) live in the webview's storage
  (`~/Library/WebKit/com.thekozugroup.stellarator` on macOS).

Environment variables are read only when the app is started from a terminal (macOS apps opened
from Finder or the Dock do not see your shell's variables):

| Variable | Effect |
| --- | --- |
| `STELLARATOR_DATA_DIR` | Use this data folder instead of the one above. |
| `STELLARATOR_DESKTOP_PORT` | The local port to try first (default 47823; `0` for any free port). |
| `HF_TOKEN` | Hugging Face token; wins over the one saved in Settings. Saving it in Settings is simpler. |
| `RUST_LOG` | Log detail, for example `debug`. |

On a Mac: `STELLARATOR_DATA_DIR=~/stellarator-data /Applications/Stellarator.app/Contents/MacOS/stellarator`.

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

- One process holds the window and the server, with no Python runtime. The page itself runs in
  the system webview's helper processes (on macOS `com.apple.WebKit.WebContent`, `.Networking`
  and `.GPU`; Activity Monitor lists them separately).
- Measured on Linux (release build, WebKitGTK on a virtual display without a GPU, the UI loaded
  and idle): the app's process had 177 MB resident, of which 38 MB was its own memory (heap and
  stacks of the window and the server together) and the rest shared library pages, 53 MB of them the
  software GL renderer that a machine with a GPU does not load. The webview's page process held
  188 MB and its network process 55 MB. macOS figures differ (WebKit lives in the system's shared
  cache) and have not been measured yet; check Activity Monitor's Memory column. Either way the
  model server is what uses the memory.
- Runs keep going in the server whether the window is in front, hidden or minimized. A hidden or
  minimized window lets the webview slow the page's timers, so the UI polls less.
- Closing the window quits the app (it is a single-window tool): runs are paused cleanly and
  resume when you open it again (Resume on the run page).

## How it works

- The server listens on `127.0.0.1` only: it is not reachable from other machines, and the macOS
  firewall does not ask about it. It prefers port 47823 so the web UI's settings, which the
  webview keeps per address, survive restarts; if that port is taken it uses any free port.
- Each launch makes a random token. The window opens `http://127.0.0.1:<port>/?launch=<token>`;
  the server answers with an HttpOnly cookie and from then on serves only requests that carry it.
  Anything else on the machine, including a web page in your browser, gets `403 Open Stellarator
  from its app.`
- Only the local server loads in the window. Links to other sites (Hugging Face, OpenRouter's
  keys page, GitHub) open in your default browser, including `target="_blank"` links.
- File pickers (Import prompts > file) use the system's open panel: AppKit's `NSOpenPanel` on
  macOS (Tauri's webview layer, wry, implements WebKit's file-upload delegate), WebView2's picker
  on Windows and GTK's on Linux.
- Only one copy runs at a time: opening the app again brings the existing window to the front, so
  two servers never share one database.
- Quitting (closing the window, Cmd+Q, Dock > Quit) stops the server before the process ends:
  in-flight requests finish and runs are paused, waiting at most 15 seconds. Installing an update
  stops it the same way (`desktop/src-tauri/src/updater.rs`).
- Menus on macOS: Stellarator (About, Check for Updates… in builds with the updater, Services,
  Hide, Quit), Edit (Undo, Redo, Cut, Copy, Paste, Select All, which the webview's text fields need
  for the keyboard shortcuts), View (Reload, full screen), Window, and Help (Stellarator on GitHub).
  Windows and Linux have no menu bar.

### The title bar on macOS

The window has no separate title bar, as in macOS 26 apps: the page runs to the top edge, the page
toolbar (title and actions) runs across the top of the content column, and the window controls
(close, minimize, zoom) sit at the top left of the sidebar panel, in the same band. Windows and
Linux keep their system title bars.

- The window uses Tauri's overlay title bar style with the title hidden, and the app gives it an
  empty unified toolbar (`use_toolbar_window` in `desktop/src-tauri/src/titlebar.rs`). That is
  what macOS 26 apps with a sidebar have, and it does two things, measured on macOS 26.6: the
  window gets 26 pt corners instead of 16 pt, and macOS centers the controls at (26, 26) pt, in the
  middle of the 52 px band. Clicks anywhere in the band still reach the page.
- A script the app adds to the page before it loads sets `window.__STELLARATOR_DESKTOP__` with the
  title bar style and the window's corner radius (26 on macOS 26, 10 on macOS 11 to 15; 8 on
  Windows and 0 on Linux, which get only the radius). The web UI (`frontend/src/lib/desktop-shell.ts`)
  then sets `data-titlebar="overlay"` and `--window-radius` on `<html>`. The CSS keeps the top
  `--titlebar-height` (52 px, equal to the toolbar's height, in `frontend/src/index.css`) of the
  sidebar free for the controls, and draws the sidebar panel 8 px inside the window with corners of
  the window's radius minus 8, so the two are concentric (DESIGN.md, "Shape"). Below a 20 px
  radius the sidebar runs flush with the window's edge instead. The Docker and browser UI never get
  the global; they use the macOS 26 values.
- The band moves the window when dragged, and a double click zooms it (always Zoom: it does not
  follow System Settings > Desktop & Dock > "Double-click a window's title bar to"): over the
  sidebar, and over the content wherever the toolbar has no button, link or field. The toolbar is
  the page's color at rest; once the page scrolls under it, its material and a divider fade in.
- When the window is not in front, the first click on the band brings it forward; drag with the
  next one (a Tauri limit, [tauri#4316](https://github.com/tauri-apps/tauri/issues/4316)).
- In full screen macOS hides the controls; the app sets `data-fullscreen` on `<html>`, the sidebar
  drops the reserved space and the toolbar stops moving the window.
- Moving and zooming are two window commands the page calls over Tauri's IPC.
  `desktop/src-tauri/capabilities/title-bar.json` allows exactly those two, for the main window, and
  only for pages from `http://127.0.0.1:<any port>`; the page can reach no other command. A test in
  `main.rs` checks this against the app's compiled permissions.

## Building it yourself

On a Mac: Xcode Command Line Tools (`xcode-select --install`), Rust (`rustup`), Node.js 22.

```sh
git clone https://github.com/thekozugroup/Stellarator && cd Stellarator
(cd frontend && npm ci)
cd desktop
npx @tauri-apps/cli@2 build
```

Run the CLI from `desktop/`: its `beforeBuildCommand` (`npm --prefix ../frontend ci` and `run
build`) is relative to that folder, and rebuilds the web UI so the server embeds a fresh copy.
The results:

- `desktop/src-tauri/target/release/bundle/macos/Stellarator.app`
- `desktop/src-tauri/target/release/bundle/dmg/Stellarator_0.2.0_aarch64.dmg`

`npm ci && npm run tauri build` in `desktop/` does the same with the CLI version pinned in
`desktop/package.json`. For an Intel build on Apple Silicon: `rustup target add
x86_64-apple-darwin`, then add `--target x86_64-apple-darwin` (or `universal-apple-darwin` for
both in one app).

While working on the app: `cargo run` in `desktop/src-tauri` builds a debug app that reads
`frontend/dist` from disk; after `npm run build` in `frontend/`, View > Reload picks up the new UI.
Logs go to the terminal.

Windows needs the Visual Studio C++ build tools; Linux needs `libwebkit2gtk-4.1-dev`,
`libxdo-dev`, `libssl-dev` and `librsvg2-dev` (see `.github/workflows/desktop.yml`).

### The icon

The icon is the Stellarator mark in white glass over a two-colour gradient, periwinkle `#5566dc`
at the top to deep indigo `#2c3684` at the bottom, in the manner of a stock macOS 26 app. The mark
is one thick ring with five soft lobes (a stellarator's five-period plasma ring seen from above);
`frontend/src/components/logo.tsx` draws the same shape. `desktop/icon-src/make_icon.py` writes
every source from one description:

| File | Used for |
| --- | --- |
| `icon-macos.svg` | macOS: Apple's 824 px rounded square with continuous corners on the 1024 px canvas, with the standard drop shadow |
| `icon.svg` | Windows, Linux and the web favicon: the same shape, nearly full-bleed |
| `icon-square.svg` | iOS home screen and the web manifest's maskable icon: full-bleed, the platform masks it |
| `Stellarator.icon` | macOS 26: an Icon Composer document (the gradient as the icon's fill, `#2c3684` to `#141a40` in the dark appearance; the mark as one white glass layer) |

On macOS 26 the system draws Liquid Glass itself, so the desktop workflow compiles
`Stellarator.icon` with Xcode's `actool` into `Assets.car`, adds it to the app and sets
`CFBundleIconName`; the app then follows the Dock's default, dark, tinted and clear icon styles.
If `actool` fails, the build goes on with the static `icon.icns` and the log says so. Before
macOS 26 the app shows `icon.icns` or the flattened icon `actool` puts in `Assets.car`. The static
icons draw the glass as a still picture: the mark in white with a little transparency, a bright
inner edge at the top and a soft shadow.

To change the icon, edit `make_icon.py` (the mark's shape is `R`, `A` and `WIDTH`; the colours are
`GRADIENT` and `GRADIENT_DARK`), then run the commands below. If the mark changed, also paste the
output of `python3 icon-src/make_icon.py --logo` into `MARK` in `frontend/src/components/logo.tsx`.

```sh
cd desktop
python3 icon-src/make_icon.py
npx @tauri-apps/cli@2 icon icon-src/icon.svg -o /tmp/icons-other
npx @tauri-apps/cli@2 icon icon-src/icon-macos.svg -o /tmp/icons-macos
cp /tmp/icons-other/{32x32.png,64x64.png,128x128.png,128x128@2x.png,icon.png,icon.ico} src-tauri/icons/
cp /tmp/icons-macos/icon.icns src-tauri/icons/
npx @tauri-apps/cli@2 icon icon-src/icon.svg --png 192,512 -o /tmp/icons-web
npx @tauri-apps/cli@2 icon icon-src/icon-square.svg --png 180,512 -o /tmp/icons-square
cp icon-src/icon.svg ../frontend/public/favicon.svg
cp /tmp/icons-web/192x192.png ../frontend/public/icon-192.png
cp /tmp/icons-web/512x512.png ../frontend/public/icon-512.png
cp /tmp/icons-square/180x180.png ../frontend/public/apple-touch-icon.png
cp /tmp/icons-square/512x512.png ../frontend/public/icon-512-maskable.png
npx @tauri-apps/cli@2 icon icon-src/icon-macos.svg --png 256 -o /tmp/icons-readme
cp /tmp/icons-readme/256x256.png ../docs/assets/icon.png
```

To check `Stellarator.icon` on a Mac, open it in Icon Composer (Xcode 26).

## Builds and releases (CI)

`.github/workflows/desktop.yml` builds the installers for macOS (Apple Silicon and Intel, both on
GitHub's Intel Mac runners, with the Apple Silicon app cross-compiled), Windows and Linux. It runs on `v*` tags, by hand (Run workflow),
and on pushes to `main` that change `desktop/`, `crates/` or `frontend/`. Every run uploads the
installers as artifacts named `stellarator-<target>`.

A push to `main` whose version has no published release yet makes a GitHub release tagged
`v<version>`, with the tag created on that commit. The workflow creates a draft, the four builds
upload their installers into it, and it is published as the latest release when the Apple
Silicon, Windows and Linux builds succeed. The Intel Mac build is optional: if it fails or runs
past its time limit, the release goes out without the Intel `.dmg`. If a required build fails, the
draft stays hidden: fix the cause and push (the next run reuses the
draft), or use Re-run failed jobs. Once a version is published, later pushes with the same version
only build the artifacts.

To release a new version:

1. Set the same version in `Cargo.toml` (`[workspace.package]`), `desktop/src-tauri/Cargo.toml` and
   `desktop/package.json`. The workflow's first job fails if the two `Cargo.toml` versions differ.
2. Update both lock files: `cargo update -w`, then `cargo update -w` in `desktop/src-tauri`.
3. Add a `## <version>` section to `CHANGELOG.md`. The release notes start with it, under
   "What's new", followed by the download table.
4. Commit and push to `main`.

The same release can also be started by Actions > Desktop > Run workflow with "Publish a release"
checked, or by pushing a matching tag (`git tag v0.3.0 && git push origin v0.3.0`). Both refuse a
version that is already published.

Without signing secrets the macOS app is ad-hoc signed (see Install). To sign it with a Developer
ID and notarize it, add these repository secrets:

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

The updater (see [Updates](#updates)) is on only in builds made with all three of these repository
secrets:

| Secret | Value |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | The private key: the contents of `stellarator.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The password you chose for it |
| `TAURI_UPDATER_PUBKEY` | The public key: the contents of `stellarator.key.pub` |

Make the key pair on your own Mac, never on a shared machine or in CI, from a clone of the
repository:

```sh
cd Stellarator/desktop
npm ci
mkdir -p ~/.tauri
npx tauri signer generate -w ~/.tauri/stellarator.key
```

It asks for a password twice; use a strong one. It writes the private key to
`~/.tauri/stellarator.key` and the public key to `~/.tauri/stellarator.key.pub`. Then add the
secrets, either on GitHub (the repository's Settings > Secrets and variables > Actions > New
repository secret, pasting each value) or with the GitHub CLI:

```sh
gh secret set TAURI_SIGNING_PRIVATE_KEY --repo thekozugroup/Stellarator < ~/.tauri/stellarator.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo thekozugroup/Stellarator
gh secret set TAURI_UPDATER_PUBKEY --repo thekozugroup/Stellarator < ~/.tauri/stellarator.key.pub
```

The second command asks for the password. Keep `stellarator.key` and its password in your password
manager or another backup, and never commit them. Every installed copy trusts only this public key:
if the private key is lost, no new update can be signed for them, and their users have to download
the next version by hand (it would carry a new key).

With the three secrets set, each build:

1. writes a config overlay (`--config`, next to the Liquid Glass one on macOS) that adds
   `plugins.updater` with the public key and the endpoint
   `https://github.com/thekozugroup/Stellarator/releases/latest/download/latest.json`, and turns on
   `bundle.createUpdaterArtifacts`. The app registers the updater only when its compiled config has
   `plugins.updater`;
2. signs the update files (`.app.tar.gz` on macOS, the `-setup.exe` and `.msi`, the `.AppImage` and
   the `.deb`) with the private key and uploads them with their `.sig` files into the draft release;
3. adds its platforms to the release's `latest.json`, which tauri-action creates.

Before the release is published, the Publish job adds any platform that has a signature in the
release but is missing from `latest.json` (two builds finishing together can overwrite each other's
entries). Publishing makes it the latest release, so installed apps find it at the endpoint above.
If the Intel Mac build failed, `latest.json` has no Intel entry and Intel Macs keep their version
until the next release.

When any of the three secrets is missing, or empty because the event gets no secrets, the build
is the same as before: no overlay, no updater in the app, no `latest.json`. To try the updater in a
local build, pass the same overlay: `npx @tauri-apps/cli@2 build --config updater.json` with the
private key and password in `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.

## Troubleshooting

| Symptom | Fix |
| --- | --- |
| macOS says the app "is damaged" or "cannot be opened" | `xattr -dr com.apple.quarantine /Applications/Stellarator.app` (see Install). |
| A dialog says "Stellarator could not start: ..." | The reason follows the colon. For more, run `/Applications/Stellarator.app/Contents/MacOS/stellarator` in Terminal and read the log. |
| Opening the app shows the existing window | Only one copy runs at a time. |
| `403 Open Stellarator from its app.` in a browser | Expected: the local server answers only the app's window. |
| A local endpoint fails to connect | Check the model server is running: `curl http://localhost:1234/v1/models` (use its port). |
| An endpoint on another computer fails on macOS 15+ | Allow Local Network access for Stellarator in System Settings > Privacy & Security > Local Network. |
| Linux: the window stays blank | Start it with `WEBKIT_DISABLE_DMABUF_RENDERER=1` (some GPU drivers) or `WEBKIT_DISABLE_COMPOSITING_MODE=1`. |
| Settings (theme, Rendered/Raw) reset after a restart | Port 47823 was taken, so the app used another address. Free the port, or set `STELLARATOR_DESKTOP_PORT`. |
| Quitting takes a few seconds | Runs are being paused; it waits at most 15 seconds. |
| Check for Updates… says "The latest release has no update for this computer." | That release has no build for your platform yet (the Intel Mac build is optional). The next release brings it. |
| Check for Updates… says "The latest release has no update information." | The latest release was made without the update signing secrets. Download the next version by hand. |
| After an update, macOS asks again for Local Network access | Builds that are not signed with a Developer ID get a new signature with each version, and macOS asks again. Allow it once more. |
| The window cannot be dragged on the first click | It was not in front; the first click brings it forward. Drag again. |
