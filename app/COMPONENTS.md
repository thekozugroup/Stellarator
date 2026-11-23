# The nsh app's components

The app's UI is Rust on nsh 0.6.1. Screens are built from the components in `src/ui/`, which
follow DESIGN.md and frontend/src/index.css (sizes, radii, colors). Use them; do not style raw
views where a component exists. Every component draws with theme tokens only (`theme().<field>`
or `crate::theme::token("name")`), so light and dark follow the system without a render.

Start the desktop app with `STELLARATOR_GALLERY=1` to see every component on one screen
(`src/screens/gallery.rs`); the Linux CI job uploads a screenshot of it (`gallery.png` in the
`nsh-linux` artifact).

## Where things are

| Path | What |
| --- | --- |
| `src/nav.rs` | The router: a stack of screens per sidebar destination |
| `src/screens/` | One module per screen; `screens::view` maps a route to its screen |
| `src/ui/` | The components below |
| `src/api.rs` | The typed API client (in-process router on desktops, `fetch` on the web) |
| `src/data.rs` | `Load<T>` and `call`: loading API data into a screen |
| `src/format.rs` | Numbers, dates, durations (frontend/src/lib/format.ts) |
| `src/shortcuts.rs` | Keyboard shortcuts |
| `src/theme/` | Tokens, type scale (`theme::text`), radii, window shape, the appearance choice |

## Components

| Component | Rust | Replaces (frontend/src/components) |
| --- | --- | --- |
| Page frame | `page("Runs")`, `page_with(\|\| Some(name))` then `.actions(f)`, `.bar_actions(f)`, `.meta(f)`, `.bottom_bar(f)`, `.wide()`, `.child(el)` | `shared/page.tsx` `Page` |
| Card | `panel(title, description, actions, content)` | `shared/page.tsx` `Panel` |
| Button | `button("Save").variant(Variant::Secondary).size(Size::Sm).leading(icon::PLUS).busy(f, "Saving…").disabled(f).on_press(f)`; `.on_card()`, `.round()`, `.muted()`, `.full_width()` | `ui/button.tsx` |
| Icon button | `Button::icon(icon::ELLIPSIS, "More actions")` (ghost, tooltip = its name) | `ui/button.tsx` size `icon*` |
| Link | `text_link(label, trailing, f)`, `external_link(label, url)` | `<a>` in forms |
| Icon | `icon(icon::PLAY, 16.0, color_fn)`; add glyphs with `tools/lucide-svg.mjs` | lucide-react |
| Text field | `text_field(signal).placeholder(..).mono().numeric().masked(f).search().width(96.0).invalid(f).trailing(el).element(&ref).on_change(f).on_submit(f)`; `text_area(signal)` | `ui/input.tsx`, `ui/textarea.tsx`, `ui/input-group.tsx` |
| Field | `field(label, control).hint(..).hint_when(f).error(signal).info(text).optional()` | `shared/field.tsx` |
| Grouped form | `form_group(rows).title(..).footnote(el).in_dialog()`; `form_row(label, control).hint(..).error(signal).optional_when(f).align_end().stacked().info(..)`; `form_item(el)`; `form_disclosure(title, summary_fn, open, rows_fn, in_dialog)` | `shared/form-group.tsx` |
| Pop-up button | `select_button(vec![option("vllm", "vLLM")], value, title_fn, placeholder, label)` | `ui/select.tsx` |
| Menu | `menu_button(label, vec![menu_entry(Some(icon::PENCIL), "Edit…", f), MenuEntry::Separator, menu_destructive(..)], Some(176.0))`; `menu_with(trigger, entries, width)` | `ui/dropdown-menu.tsx` |
| Combobox | `combobox_field(options, value, placeholder, label)` | `ui/command.tsx` (as a combobox) |
| Segmented control | `segmented(options, signal, label, fill)`, `segmented_with_get(options, get, set, label, fill)`, `segmented_small(..)` (12 px) | `ui/toggle-group.tsx` with `spacing={0}` |
| Line tabs | `line_tabs(vec![(value, "Samples", Some(count_fn))], signal)` | `ui/tabs.tsx` variant line |
| Toggle | `toggle(label, signal)` | `ui/toggle.tsx` |
| Checkbox | `checkbox(label, signal)`, `checkbox_with(label, get, set)` | `ui/checkbox.tsx` |
| Radio | `radio_group(options, signal)` | `ui/radio-group.tsx` |
| Switch | `switch(label, signal)` | (none in the web UI yet) |
| Tooltip | `view().tooltip(tooltip_bubble("…"))` | `ui/tooltip.tsx` |
| Key | `kbd(shortcuts::label("mod+["))` | `ui/kbd.tsx` |
| Copy button | `copy_button(value, "Copy base URL")` | `shared/copy-button.tsx` |
| Info tip | `info_tip(label, explanation)` | `shared/info-tip.tsx` |
| Status | `pill(Tone::Positive, "Reachable")`, `pill_small(..)`, `status_badge(tone, glyph, word)`, `run_status_pill(s)`, `run_status_badge(s)`, `sample_status_pill(s, finish_reason)` | `shared/status-pill.tsx`, `RunStatusBadge`, `SampleStatusPill` |
| Chips | `chip(label).mono().leading(plasma(..)).on_press(f).selected(f)`; `chip_row(vec)` | `shared/chip.tsx` |
| Tiles | `tiles(vec![tile("Models", f).unit(..).span().child(bar).background(dialog_muted)])` | `shared/tiles.tsx` |
| Pixel tile | `plasma(seed, cells, size, radius, deep)`; `plasma_cells` gives the same colors as the web | `shared/plasma.tsx` |
| Progress | `progress_bar(done, failed, total, width)`, `progress_bar_full(..)` | `shared/progress-bar.tsx` |
| Table | `header(&COLUMNS)`, `row(cells, last_fn)`, `cell(Column::fixed("Status", 160.0), el)` | `ui/table.tsx` |
| Block states | `error_block(message, Some(retry))`, `empty_block(children, action)`, `empty_text(..)`, `bar(w, h)` | `shared/block-states.tsx` |
| Dialog | `dialog(open, label, \|\| content).width(DialogWidth::Xl).gap(0.0)`; `dialog_header`, `dialog_description`, `dialog_footer(vec![buttons])` | `ui/dialog.tsx` |
| Alert | `alert(open, \|\| Alert { .. }, busy_fn, error_fn, on_action)` | `ui/alert-dialog.tsx` |
| Side sheet | `side_sheet(open, label, \|\| content)`; `sheet_header`, `sheet_close(open)` | `ui/sheet.tsx` (side right) |
| Toasts | `toast_success(msg)`, `toast_error(msg)`, `toast_info(msg)` (the shell holds the toaster) | `ui/sonner.tsx` |
| Action bar | `page(..).bottom_bar(\|\| action_bar(vec![..]).into_element())` | `shared/action-bar.ts` |
| Model text | `model_text(\|\| text, None)`, `text_mode_toggle()` | `shared/model-text.tsx`, `shared/markdown.tsx` |
| Materials | `material_bar(view())`, `material_thick(..)`, `material_float(..)`, `dialog_muted` | index.css `material-*` |
| Type | `theme::text::{body, label, meta, title, mono, display}` | Tailwind text classes |

Text fields are 13 px, 16 px below 768 px (mono fields stay 12 px), as the web UI's inputs.
Inside a dialog or sheet, `muted` cards read as `dialog_muted` (the ground in light, the raised
tone in dark): pass `.in_dialog()` to form groups and `.background(dialog_muted)` to tiles.

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
  or a dialog is not cancelled when the popup closes. Do not use a bare `spawn` from a popup's
  handler: it stops when the popup goes.

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
| A menu's trigger must take the press itself | `menu_button` uses a button without a tooltip |
| The Linux window has a minimum width of 900 px | Phone layouts are checked in the web build at 390 px |
| On Linux, Escape in a focused text field only leaves the field | A second Escape (or the X, or the scrim) closes the dialog |
| No stacked toast deck | Toasts are a list at the bottom right (three at most, 4 s each) |
| `TextView` highlights code with its own palette | Code blocks keep nsh's colors, not index.css's three hues |
| Crossing 768 px rebuilds the shell | The open screen is made again (an open dialog closes); its scroll position is kept |
