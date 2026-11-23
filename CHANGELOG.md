# Changelog

Each version's section is copied into its GitHub release under "What's new".

## Unreleased

### Tool use from MCP servers

- Runs can give the model tools from MCP servers. Stellarator runs each call on its server, sends
  the result back, and repeats until the model answers or a limit on rounds is reached. Each
  sample keeps its whole trace: every assistant turn with its thinking and tool calls, every tool
  result, and the final answer. See [docs/tools.md](docs/tools.md).
- MCP servers: add them by pasting the JSON from Claude Desktop, Cursor, Windsurf or VS Code, or
  one at a time; stdio and streamable HTTP. A check lists their tools. Env and header values are
  secrets, returned as hints only and kept when a server is edited.
- Per run: pick servers and tools, the number of rounds (default 8), a timeout per call (60 s), a
  limit on result length (20,000 characters) and `tool_choice` (`auto` or `required`). The exact
  tool definitions are stored with the run; two servers' tools of the same name become
  `<server>__<tool>`.
- Bad arguments, unknown tools, tool errors and timeouts go back to the model as the tool's result.
  A crashed MCP server is reconnected once. An endpoint without tool calling pauses the run with
  the flags to add. Samples out of rounds are truncated with finish reason `tool_rounds`.
- Exports: chat messages with `tool_calls`, `tool` messages and a `tools` column (TRL, Axolotl);
  ShareGPT with `function_call` and `observation` turns (LLaMA-Factory); raw with the trace; DPO
  and KTO with whole trajectories. Alpaca and prompt-completion are refused for runs with tools.
  Parquet marks tool arguments and definitions as JSON, so `datasets` reads them as objects.
- In server mode (Docker, `stellarator-server`), stdio MCP servers, which start commands, need
  `STELLARATOR_ALLOW_STDIO_MCP=1`. The desktop app allows them.
- The mock server serves an MCP server at `/mcp` and on stdio (`stellarator-mock mcp-stdio`), and
  tool-calling models (`mock-tools`, `mock-tools-loop`, `mock-no-tools`).

## 0.4.0

### A native app

- The desktop app is rewritten in Rust on nsh (Native Shell). Its window is drawn with each
  system's own toolkit (AppKit on macOS, Win32 on Windows, GTK 4 on Linux) instead of a web page
  in the system's web view, and the server runs inside the same process with no network port.
  Every screen, dialog and export works as in 0.3, with the same design.
- The browser UI that Docker and `stellarator-server` serve is the same code compiled to
  WebAssembly. Each screen has its own address (`/runs/4`, `/prompts/import`, `/preferences/1`):
  back and forward move through the app, and a link opens the screen it names.

### Smaller in memory

- One process and no web view. Idle on the Runs screen, measured in CI: a 16 MiB footprint on
  macOS 26 and 21 MiB of private memory on Windows. 0.3 also ran the system web view's processes;
  on Linux those held about 240 MB more.
- The installers are larger, because the app carries its own fonts and drawing code instead of
  using the system's web view: the Apple Silicon `.dmg` is 32.2 MB (16.0 MB in 0.3.1), the
  Windows `-setup.exe` 19.6 MB (10.5 MB). The AppImage is smaller, 28.0 MB instead of 93.5 MB, as
  it no longer bundles WebKitGTK.

### New installers

- macOS 14 or later, Apple Silicon and Intel. Windows 10 or 11: the `-setup.exe` and the `.msi`
  both install for your account, without administrator rights, with a Start menu shortcut. Linux
  with GTK 4.14 or later (Ubuntu 24.04+, Debian 13+, Fedora 40+): the `.deb` and an AppImage.
  macOS 12 and 13, Ubuntu 22.04 and Debian 12 are no longer supported.
- No WebView2 or WebKitGTK is needed.

### Desktop behavior

- One copy per data folder. Opening the app again brings its window forward, and a second
  process on the same data folder (another copy, or `stellarator-server` started on it) says
  "Stellarator is already open" and quits, so two processes never write one database.
- The window opens where you left it: size, position, and maximized or full screen. It shows once
  it is ready, with its final corners.
- The macOS menu bar has Settings… (⌘,) in the app menu and Back (⌘[) in View; the Window menu
  lists the window and macOS's tiling items.
- Updates are signed with new keys and read `nsh-latest.json`.

### Fixes

- Stopping an import that finishes before the stop reaches the server keeps the prompt set: the
  server now refuses to delete a set whose import is done (`POST /api/prompt-sets/{id}/stop`).

### Coming from 0.3

- 0.3's updater cannot install 0.4.0: download it by hand from this release. Quit 0.3 first.
- Your runs, prompt sets, endpoints and settings are in the same data folder and open as they are.
  The window's size and the theme start from their defaults.
- On Windows, uninstall 0.3 from Settings > Apps: the new installers do not remove it. On macOS,
  dragging the app to Applications replaces 0.3; on Linux the `.deb` installs over it.

## 0.3.1

### Fixes

- Windows: the app opens its database again. The data folder's full path (`\\?\C:\...`) was read
  as a database URL, so the `?` started a query and the server stopped at launch with "unknown
  query parameter". The server now opens the database file by its path.

## 0.3.0

### A new look

- A new design system (DESIGN.md): Inter for text and Roboto Mono for data, an ink near-black,
  neutral grays, a periwinkle accent on data, coral for attention and green for success, and cards
  set apart by tone instead of borders.
- Detail screens open with a large title, a row of chips (status, model, endpoint, prompt set) and
  the facts as tiles; the toolbar shows where you are ("Runs / Distill gsm8k").
- Frosted, tinted materials on the chrome that floats over content: the toolbar once you scroll,
  menus, dialogs, sheets, toasts, the phone tab bar, and the action bars of New run, Import and
  preference review, which now float above the content.
- A run's chart counts back from now, cuts out paused time, and flags the change in tokens per
  second after you change seqs. The sequences in flight read like a log.
- Endpoints and prompt sets get a small pixel-field tile.

### Corners that fit the window

- On macOS 26 the window now has the larger corners of macOS 26 apps with a toolbar (26 pt
  instead of 16 pt), and the traffic lights sit where macOS puts them. The sidebar, dialogs,
  sheets and everything inside them are concentric with those corners. On macOS 15 and earlier,
  Windows and Linux the sidebar runs flush with the window's edge.

### New mark and app icon

- A simpler, monochrome mark, and an app icon styled like a stock macOS app: a two-color gradient
  with the mark in frosted glass.

## 0.2.1

### Looks and feels like a Mac app

- A floating sidebar and a toolbar at the top of each page (title, back, actions), as in macOS 26.
  The toolbar frosts once content scrolls under it.
- Menus, popovers, tooltips, dialogs, sheets and toasts use a frosted blur that stays legible over
  anything behind it. It turns solid with Increase Contrast or Reduce Transparency.
- macOS sizes for mouse and trackpad (13 px text, 28 px controls), iOS sizes on touch (44 pt
  targets). The system font on Apple devices.
- Lists in the macOS table style; New run and Settings laid out like System Settings.
- On phones: a floating tab bar, large titles, and "+" and "…" in the top bar.
- The macOS window has no separate title bar: the sidebar runs to the top with the traffic lights.

### New app icon

- Two white glass coils over a plasma gradient. On macOS 26 the system draws it in Liquid Glass and
  follows the Dock's dark, tinted and clear styles.

### Importing prompts

- Simple mode (the default): paste a Hugging Face link or name and it loads by itself, picks the
  prompt and system prompt columns, the split, and the first user turn of conversations, removes
  duplicates, and lists what it picked and what it left out.
- Advanced mode shows every setting. Every field has an info button that explains it.
- A dataset without a split called "train" uses the first one named like it (such as train_sft).

### Updates

- In-app updates from GitHub Releases: a check at start and every 6 hours, a dialog to install and
  restart, and Check for Updates… in the macOS app menu. On in builds made with the signing key;
  see docs/desktop.md.

### Fixes

- An equation on its own line (`$$ … $$`) renders as display math instead of small inline math.

## 0.2.0

- The first release: the desktop app for macOS, Windows and Linux.
