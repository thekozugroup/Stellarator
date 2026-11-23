# The app's components

The app's UI is Rust on nsh 0.6.1. Screens are built from the components in `src/ui/`, which
follow DESIGN.md (sizes, radii, colors). Use them; do not style raw
views where a component exists. Every component draws with theme tokens only (`theme().<field>`
or `crate::theme::token("name")`), so light and dark follow the system without a render.

Start the desktop app with `STELLARATOR_GALLERY=1` to see every component on one screen
(`src/screens/gallery.rs`); the Linux CI job uploads a screenshot of it (`gallery.png` in the
`nsh-linux` artifact).

Module comments that name a `frontend/...` file point to the React UI the screen was ported from.
It was removed in 0.4.0; it is in the repository at tag `v0.3.1`.

## Where things are

| Path | What |
| --- | --- |
| `src/nav.rs` | The router: a stack of screens per sidebar destination |
| `src/screens/` | One module per screen; `screens::view` maps a route to its screen |
| `src/ui/` | The components below |
| `src/api.rs` | The typed API client (in-process router on desktops, `fetch` on the web) |
| `src/data.rs` | `Load<T>` and `call`: loading API data into a screen |
| `src/format.rs` | Numbers, dates, durations |
| `src/shortcuts.rs` | Keyboard shortcuts |
| `src/desktop.rs` | Around the window: one instance, showing the window at launch, the macOS menu bar, the start-failure dialog |
| `src/updater.rs` | In-app updates (on in builds with the update keys; docs/desktop.md) |
| `packaging/` | The `.deb`, AppImage and MSI recipes (`.github/workflows/release-nsh.yml` runs them) |
| `src/theme/` | Tokens, type scale (`theme::text`), radii, window shape, the appearance choice |

## Components

| Component | Rust | Ported from (the React UI's `frontend/src/components`, removed in 0.4.0) |
| --- | --- | --- |
| Page frame | `page("Runs")`, `page_with(\|\| Some(name))` then `.actions(f)`, `.bar_actions(f)`, `.meta(f)`, `.bottom_bar(f)`, `.wide()`, `.bottom_bar_below_lg()` (the bar floats below 1024 px only), `.child(el)` | `shared/page.tsx` `Page` |
| Card | `panel(title, description, actions, content)`; `panel_tight(..)` (body padding 8 px, 4 px on top) | `shared/page.tsx` `Panel` |
| Button | `button("Save").variant(Variant::Secondary).size(Size::Sm).leading(icon::PLUS).busy(f, "Saving…").disabled(f).on_press(f)`; `.on_card()`, `.round()`, `.muted()`, `.full_width()` | `ui/button.tsx` |
| Icon button | `Button::icon(icon::ELLIPSIS, "More actions")` (ghost, tooltip = its name) | `ui/button.tsx` size `icon*` |
| Link | `text_link(label, trailing, f)`, `external_link(label, url)` | `<a>` in forms |
| Icon | `icon(icon::PLAY, 16.0, color_fn)`; add glyphs with `tools/lucide-svg.mjs` | lucide-react |
| Text field | `text_field(signal).placeholder(..).placeholder_with(f).mono().numeric().tabular().centered().masked(f).search().width(96.0).invalid(f).trailing(el).element(&ref).on_change(f).on_blur(f).on_submit(f)`; `text_area(signal).height(80.0)`, `.mono_body()` (Roboto Mono at the regular 13/16 px, for text the user writes) (`numeric` is right-aligned, `tabular` left-aligned figures) | `ui/input.tsx`, `ui/textarea.tsx`, `ui/input-group.tsx` |
| Field | `field(label, control).hint(..).hint_when(f).error(signal).info(text).optional()` | `shared/field.tsx` |
| Grouped form | `form_group(rows).title(..).footnote(el).in_dialog()`; `form_row(label, control).hint(..).hint_element(f).error(signal).optional_when(f).align_end().stacked().info(..)` (`hint_element` makes the hint a sentence with a link; the error takes its place); `form_item(el)`; `form_disclosure(title, summary_fn, open, rows_fn, in_dialog)`; `form_group(rows).dynamic_rows(f)` (rows after the fixed ones that come and go, with hairlines only for the rows given) | `shared/form-group.tsx` |
| Pop-up button | `select_button(vec![option("vllm", "vLLM")], value, title_fn, placeholder, label)`; full width with its own fill: `select_field(options, value, title_fn, placeholder, label, fill)` (`on-dialog-control` in a dialog's group); `select_row(options, get, set, label).fill().mono().title(f)` (read with `get`, as wide as its column with `.fill()`, on the secondary fill) | `ui/select.tsx` |
| Menu | `menu_button(label, vec![menu_entry(Some(icon::PENCIL), "Edit…", f), MenuEntry::Separator, menu_destructive(..)], Some(176.0))`; `menu_with(trigger, entries, width)`; `menu_with_open(trigger, entries, width, open)` keeps `open` true while it shows (keys that must stand down) | `ui/dropdown-menu.tsx` |
| Combobox | `combobox_field(options, value, placeholder, label)` | `ui/command.tsx` (as a combobox) |
| Rich pop-up button | `RichSelect { items, value, on_change, shown, render_item, placeholder, accessibility, invalid, element }` (`ui/pickers.rs`): the screen draws the trigger and the rows (a tile, a name, a muted detail); full width, on a card | `ui/select.tsx` with custom items |
| Typed combobox | `TypedCombobox { items, value, on_change, on_query, placeholder, accessibility, empty, mono, disabled, invalid, element }` (`ui/pickers.rs`): a field that filters the listed items and lets the screen offer what was typed as one more | `runs/model-combobox.tsx` |
| Number stepper | `number_stepper(signal, "seqs", 1, 4096).invalid(f).on_change(f).on_blur(f).on_submit(f).element(&ref)` (`ui/stepper.rs`): a whole-number field with minus and plus; the buttons clamp, Up and Down step | `runs/number-stepper.tsx` |
| Seqs choices | `seqs_choices(get, set)`, `SEQS_CHOICES` (`ui/stepper.rs`): 1, 4, 8, 16, 32, 64, 128, 256 as one segmented row (filling its column below 640 px with 4 px label padding, as wide as its segments with 8 px from 640 px); none chosen for another number | `runs/seqs-choices.tsx` |
| Segmented control | `segmented(options, signal, label, fill)`, `segmented_with_get(options, get, set, label, fill)`, `segmented_small(..)` (12 px), `segmented_mono(options, get, set, label)` (mono 12 px labels), `segmented_padded(options, get, set, label, fill, pad)` (another label padding) | `ui/toggle-group.tsx` with `spacing={0}` |
| Line tabs | `line_tabs(vec![(value, "Samples", Some(count_fn))], signal)` | `ui/tabs.tsx` variant line |
| Toggle | `toggle(label, signal)` | `ui/toggle.tsx` |
| Checkbox | `checkbox(label, signal)`, `checkbox_with(label, get, set)` | `ui/checkbox.tsx` |
| Radio | `radio_group(options, signal)` | `ui/radio-group.tsx` |
| Switch | `switch(label, signal)` | (none) |
| Tooltip | `view().tooltip(tooltip_bubble("…"))` | `ui/tooltip.tsx` |
| Key | `kbd(shortcuts::label("mod+["))` | `ui/kbd.tsx` |
| Copy button | `copy_button(value, "Copy base URL")` | `shared/copy-button.tsx` |
| Info tip | `info_tip(label, explanation)` | `shared/info-tip.tsx` |
| Status | `pill(Tone::Positive, "Reachable")`, `pill_small(..)`, `status_badge(tone, glyph, word)`, `run_status_pill(s)`, `run_status_badge(s)`, `sample_status_pill(s, finish_reason)` | `shared/status-pill.tsx`, `RunStatusBadge`, `SampleStatusPill` |
| Chips | `chip(label).mono().leading(plasma(..)).on_press(f).selected(f)`; `chip_row(vec)` | `shared/chip.tsx` |
| Tiles | `tiles(vec![tile("Models", f).unit(..).span().child(bar).background(dialog_muted)])` | `shared/tiles.tsx` |
| Pixel tile | `plasma(seed, cells, size, radius, deep)`; `plasma_cells` gives the colors the React UI gave the same seed | `shared/plasma.tsx` |
| Progress | `progress_bar(done, failed, total, width)`, `progress_bar_full(..)` | `shared/progress-bar.tsx` |
| Table | `header(&COLUMNS)`, `row(cells, last_fn)`, `cell(Column::fixed("Status", 160.0), el)`, `Column::share("Name", 1.1)` (a share of the free space) | `ui/table.tsx` |
| Block states | `error_block(message, Some(retry))`, `empty_block(children, action)`, `empty_text(..)`, `bar(w, h)` | `shared/block-states.tsx` |
| Dialog | `dialog(open, label, \|\| content).width(DialogWidth::Xl).gap(0.0).busy(f)` (`Xs` 320, `Sm` 384, `Md` 448, `Xl` 576, `FourXl` 896 px; while `busy` holds, Escape, the X and the scrim do nothing); `dialog_header`, `dialog_description`, `dialog_footer(vec![buttons])` | `ui/dialog.tsx` |
| Alert | `alert(open, \|\| Alert { .. }, busy_fn, error_fn, on_action)` | `ui/alert-dialog.tsx` |
| Side sheet | `side_sheet(open, label, \|\| content)` (448 px), `side_sheet_width(open, label, 720.0, \|\| content)`; `sheet_header`, `sheet_close(open)` | `ui/sheet.tsx` (side right) |
| Toasts | `toast_success(msg)`, `toast_error(msg)`, `toast_info(msg)`, `toast_error_with(msg, second_line)` (the shell holds the toaster) | `ui/sonner.tsx` |
| Action bar | `page(..).bottom_bar(\|\| action_bar(vec![..]).into_element())`; in a state's tint: `action_bar_tinted(vec![..], "review-bar-tint")` | `shared/action-bar.ts` |
| Model text | `model_text(\|\| text, None)`, `text_mode_toggle()`; sized or muted: `model_text_with(\|\| text, None, ModelTextStyle::small().muted_when(f))` (13 px on 18 px, a card's `text-sm`) | `shared/model-text.tsx`, `shared/markdown.tsx` |
| Materials | `material_bar(view())`, `material_bar_tinted(view(), token)`, `material_thick(..)`, `material_float(..)`, `dialog_muted` | index.css `material-*` |
| Type | `theme::text::{body, label, meta, title, mono, display}` | Tailwind text classes |

Screens share one more piece outside `ui/`: the export dialogs' layout, `screens::export`
(`components/export/export-dialog-shell.tsx`). `export_dialog(open, ExportSpec { .. }, || options)`
draws the preview, the Hugging Face push and Download; the options are built with
`choice_group`, `option_group` + `check_option` and `file_type_group`. Its module comment shows how
to use it; `screens/export/run.rs` (a run's export) is the worked example.

Text fields are 13 px, 16 px below 768 px (mono fields stay 12 px).
Inside a dialog or sheet, `muted` cards read as `dialog_muted` (the ground in light, the raised
tone in dark): pass `.in_dialog()` to form groups and `.background(dialog_muted)` to tiles.
Cards there have 10 px corners (26 - 16); form groups `in_dialog()` take them.

## Links between screens

| Call | What |
| --- | --- |
| `screens::endpoints::open_edit(id, EditFocus::Key)` | Switches to Endpoints and opens that endpoint's edit dialog with the key (`EditFocus::Url`: the URL; `EditFocus::None`: the name) focused. Closing the dialog, saved or not, goes back to the destination the call came from |
| `screens::endpoints::status::endpoint_trouble(Some(&ep))` | `Some(Trouble::KeyRejected)` or `Some(Trouble::Unreachable)` when the last check failed; `.word()` and `.field()` (the `EditFocus` that fixes it) |
| `screens::new_run::open(Prefill { endpoint, prompt_set, from })` | Shows New run starting from an endpoint, a prompt set, or a run to copy ("Run again…": `from`) |

## Adding a screen

1. Add the route to `nav::Route` (`src/nav.rs`), and in the same file its destination
   (`Route::destination`) and its label (`Route::label`). A route that is a destination's root is
   a top-level screen; any other is a detail screen, pushed on that destination's stack.
2. Write `src/screens/<name>.rs` (or a folder with `mod.rs`) with `pub fn screen(..) -> impl
   IntoElement` returning a `page(..)`. Look at `screens/endpoints/` for a list, a dialog, a
   sheet and an alert.
3. Add the arm in `screens::view` (`src/screens/mod.rs`).
4. Open it with `nav::push(Route::Run { id }, Some(run_name))`. The label names it in the
   breadcrumb and the phone's back button until the screen calls `nav::set_label(name)` once its
   data has loaded. `nav::replace` swaps the shown screen (New run becomes the run);
   `nav::back()` goes back; `nav::open(Destination::Runs)` switches destination.

The page builder does the rest: the breadcrumb ("Runs / Distill gsm8k") and the display title on
detail screens, the actions in the toolbar (from 768 px on top-level screens, from 1024 px on
detail screens), the phone bar below 768 px (the back button or Settings, then `bar_actions`),
the frosted toolbar once the content scrolls under it, and the scroll position kept per screen
entry. Cmd+[ (Ctrl+[) goes back everywhere.

Closures passed to `.actions`, `.meta` and `.bar_actions` are run again when the signals they
read change, and when the window crosses 1024 px.

## Calling the API

```rust
let runs = signal(Load::<Vec<RunOut>>::Loading);
let reload = move || call(|api| async move { api.runs().await }, move |r| runs.set(Load::from(r)));
reload();
// Read it in views: runs.with(|l| ...). Refresh on a timer: let _ = set_interval(dur, reload);
```

- Add typed methods to `Api` in `src/api.rs` (`get`, `post`, `put`, `patch`, `delete` take a
  path and a `Serialize` body). Use the server's types from `stellarator_api` (`api::types`);
  types the server only serializes (`&'static str` fields) need a local `Deserialize` copy, as
  `EndpointPreset` and `Settings` have.
- `ApiError` has the server's `detail` as `message`, the HTTP `status`, and per-field messages in
  `fields` (show them under their fields).
- `call` runs the request in a task owned by the open screen: a request started from a menu item
  or a dialog is not cancelled when the popup closes. Do not use a bare `spawn` from a handler: it
  stops when the popup goes, or at once when the handler's own write made its region again (below).
- `done` runs as one batch, whenever the request ends while the screen is open. Follow the rules
  in the next section for what it may write.

## Handlers, requests and regions that go

nsh runs a press (or change, submit, drop) handler under the owner of the region that made the
control, and does not batch it: each signal it writes runs that signal's bindings before the
handler goes on. When the handler writes a signal that the enclosing `dynamic` or `when` reads
(Retry setting `Loading`, Next setting `page`, Clear pair changing the marks), the region is made
again at once and the handler's owner is disposed under it (nsh-core `reactive.rs`, `dispose`: the
current owner becomes `DISPOSED`). For the rest of the handler `use_context` finds nothing, a task
or timer it starts (`spawn`, `set_timeout`, `request_animation_frame`) is cancelled at once, and
the old region's signals panic when written ("written after its owner was disposed"; on the web
the panic stops the app). A request's answer meets the same signals after a dialog closed. The
rules, each checkable in review:

1. Start every request with `data::call`. It runs the request under `data::screen_owner()`: the
   screen from the context, else the open screen `own_calls` keeps, never a disposed owner. So
   the order of the writes and the call in a handler does not matter. Start a request before
   navigating away in the same handler, not after.
2. A timer or task a handler starts that must outlive the pressed control runs under the owner of
   the state it writes: `screen_owner().run(|| set_timeout(..))`, or an owner kept beside that
   state (`SeqsChange` in `screens/run/seqs.rs` keeps the wait before a seqs change is sent).
   Focus a field that a press shows from the region that makes the field (a
   `request_animation_frame` there), not from the press.
3. `done` runs as one batch, so a write that makes a region again (the screen's `load`) does not
   dispose, halfway through `done`, the region's signals `done` writes next. It writes only
   signals that outlive the request: made in the screen's body, or beside a dialog (next to its
   `open`), not in its content or in a `dynamic`. Any other signal it writes with `try_set` or
   `data::SignalExt::try_update`, and reads with `try_get`.
4. A dialog that saves keeps its busy flag beside its `open` and passes it to `Dialog::busy`;
   its Cancel is disabled with the same flag. It cannot close until the answer comes, so the
   answer finds the form (its field errors) and a second save cannot start. Alerts take `busy`
   already. A request whose answer may be dropped (Test connection) leaves Cancel free and writes
   with `try_set`.
5. A region reads only what decides its structure. A value that changes often (a running run's
   counts, the text typed in a row) goes in a memo of the fields the region uses, or in a
   binding inside it, so the region is not made again while the user works in it (an open "…"
   menu, a focused field). Rows that come and go read their ids alone.

## Keyboard shortcuts

`shortcuts::bind("j", f)` in a screen's body binds a key while the screen is shown. Plain letters
do not fire while a text field has the focus. App-wide keys go in `shortcuts::install`.

## Building and screenshots

All builds go to a target folder outside the repository (disk is limited):

```bash
cd app
CARGO_TARGET_DIR=/tmp/app-target CARGO_PROFILE_DEV_DEBUG=0 cargo test --lib   # unit tests
CARGO_TARGET_DIR=/tmp/app-target CARGO_PROFILE_DEV_DEBUG=0 cargo build --bin stellarator
```

Linux under Xvfb (the window fills the virtual screen; dark mode through GTK's setting):

```bash
Xvfb :91 -screen 0 1440x900x24 -nolisten tcp &
mkdir -p /tmp/xdg-dark/gtk-4.0 && printf '[Settings]\ngtk-application-prefer-dark-theme=1\n' > /tmp/xdg-dark/gtk-4.0/settings.ini
DISPLAY=:91 GSK_RENDERER=cairo STELLARATOR_DATA_DIR=/tmp/stell-data /tmp/app-target/debug/stellarator &
python3 ci/seed.py /tmp/stell-data        # after the first launch made the database; then start again
DISPLAY=:91 xdotool search --pid $! | tail -1 | xargs -I{} env DISPLAY=:91 xdotool windowsize {} 1440 900
DISPLAY=:91 xdotool mousemove 110 140 click 1   # the sidebar's Endpoints row
DISPLAY=:91 import -window root shot.png
# Dark: add XDG_CONFIG_HOME=/tmp/xdg-dark to the app's environment.
```

The browser build: `nsh package --target web --out /tmp/web-out` (with the same environment), then
serve `/tmp/web-out/Stellarator` with `/api` proxied to a `stellarator-server`
(`STELLARATOR_BIND=127.0.0.1:8130 STELLARATOR_DATA_DIR=/tmp/stell-data`), and take screenshots
with Playwright at 1440×900 and 390×844, `colorScheme` light and dark. CI does both in
`.github/workflows/nsh.yml`.

## What nsh does not do (and what the app does instead)

| Limit in nsh 0.6.1 | In the app |
| --- | --- |
| No backdrop blur on Linux (GTK) | `material::*` draws the opaque fallback fill there; blur elsewhere |
| No `position: sticky` | The toolbar and the action bar float over the page's scroll view; table headers scroll with the rows |
| No number or date formatting by locale | `format.rs` keeps the grouping and decimal marks of the common languages and the system's hour cycle; words stay English |
| A popup's tasks stop when it closes | `data::call` runs requests under the screen |
| A handler is not batched: a write its region reads disposes the handler's owner while it runs | `data::screen_owner()` finds the open screen anyway; see "Handlers, requests and regions that go" |
| Escape closes a `modal` natively (the default `close_on_escape`), whatever the app decides | `ui::overlay` sets `close_on_escape(false)`: Escape reports `on_cancel` and the dialog's or alert's `cancel` decides (a busy one stays) |
| A menu's trigger must take the press itself | `menu_button` uses a button without a tooltip |
| The Linux window has a minimum width of 900 px | Phone layouts are checked in the web build at 390 px |
| On Linux, Escape in a focused text field only leaves the field | A second Escape (or the X, or the scrim) closes the dialog |
| No stacked toast deck | Toasts are a list at the bottom right (three at most, 4 s each) |
| `TextView` highlights code with its own palette | Code blocks keep nsh's colors, not the three hues the React UI used |
| `mask-image` is not drawn on Windows (nsh's note) | A clamped prompt's fade shows there as a plain cut |
| No call to scroll a page to an element | Paging in a prompt set keeps the scroll position (the React UI scrolled to the list's top) |
| A select's items are plain titles | A column select says "question · text" in one face (the React UI set the name in mono) |
| A task started from a handler after a signal that rebuilds the pressed control is owned by the control, which is gone | `data::call` runs it under the open screen whatever the order; timers go under `screen_owner()` |
| Crossing 768 px rebuilds the shell | The open screen is made again (an open dialog closes); its scroll position is kept |
| A screen's key bindings still fire while a dialog or menu of the same window is open | The screen checks its own dialogs' signals (and `menu_with_open`) before acting, as the preference review does |
| `TextView`'s part styles are read once | `model_text_with` makes the text view again when its color changes (a response marked rejected) |
| No `scrollIntoView` with a margin | The preference review reveals a zero-height mark placed the toolbar's height above the prompt |
| `TextViewParts` given with `provide_context` do not reach a text view made in a `dynamic` | `model_text_with` also passes the size, line height and color as the root's classes |
| `TextView` reads `$x$` as a formula (single dollars are prices more often than math) | `normalize_math` escapes single dollars outside code |
| `Plot`'s marks take dashes only from a JS style, and its axes size themselves and keep the time labels inside the plot | The run's throughput chart is a canvas with the labels, the flag and the readout as views over it |
| No CSS grid | Grid tiles take their width from the row's `on_layout` (`tile_width` in `screens/run/progress.rs`); a panel beside another takes its height with an absolutely placed scroll view |
| No `position: sticky` inside a dialog's scroll area | The export preview moves down by the scroll offset (`translate_y`), held inside the options' height |
| No `text-wrap: balance` | Long titles wrap where the line is full |
