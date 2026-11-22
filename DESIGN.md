# Design system

## Overview

An Operate tool, used in short glances during long runs. Quiet chrome, color on data, large type
for the one fact a screen answers. Light and dark themes follow the system setting; both are
designed, neither is an inversion.

The system is taken from the owner's reference boards (2026-11): Inter with Roboto Mono, an ink
near-black, neutral grays, a periwinkle accent with coral and green as signal colors, generous
radii, tone-separated surfaces without borders, mono for timestamps, IDs and axis labels, and
pixelated gradient fields as the brand texture. Where the boards and the earlier macOS work meet,
the window and its sidebar follow macOS 26 and every nested corner is concentric with the window.

Identity elements:

- The mark: a five-lobed ring (a stellarator's plasma cross-section), drawn as one bold shape in
  ink or white. On the app icon it sits in white glass on a pixel field.
- The pixel field (Plasma): a coarse grid of square cells stepping through two or three palette
  colors with a little ordered dither, like a low-resolution image of plasma. It is the only
  gradient in the product. It appears on the app icon, on the tiles that stand for endpoints and
  prompt sets, and in empty states. Never behind text and never as a page background.
- The signature element: on a run, throughput (tokens per second) drawn over the seqs setting as a
  step line, so the effect of changing concurrency is visible. Beside it, the sequences in flight
  as a log: mono time, a colored state word, the prompt.
- Trade words in labels: seqs, thinking trace, truncated, chosen, rejected.

Platform: the layout follows macOS 26 and iOS 26 (Apple HIG): a floating sidebar, a toolbar per
screen, a floating tab bar on phones, iOS metrics on touch. Floating chrome uses a tinted frosted
material in the manner of Android 17's blurred surfaces, opaque enough to read over anything
(Materials). The same UI runs in the macOS app, in a browser on any desktop, and on phones.

## Color

Tokens live in `frontend/src/index.css` (OKLCH there; hex here for reading). Components use the
roles, never raw values.

| Role | Token | Light | Dark | Use |
| --- | --- | --- | --- | --- |
| Ground | `--background` | `#ffffff` | `#0b0b0b` | the window and page |
| Shell | `--sidebar` | `#f5f5f6` | `#141414` | the floating sidebar panel |
| Surface | `--muted` | `#f4f4f5` | `#171717` | cards, tiles, wells, table header band, filled fields |
| Raised | `--secondary` | `#ececee` | `#232323` | secondary buttons, chips, segmented track |
| Highlight | `--accent` | `#ececee` | `#262626` | hovered and highlighted rows and menu items, the selected sidebar row |
| Popover | `--popover` | `#ffffff` | `#1c1c1c` | menus, popovers, dialogs, sheets (under the material) |
| Ink | `--foreground` | `#0b1515` | `#f4f4f4` | text and strong marks; light ink is near-black with a teal cast |
| Muted ink | `--muted-foreground` | `#5e5e5e` | `#9a9a9a` | secondary text, at least 4.5:1 on ground, surface and raised |
| Hairline | `--border` | `#e7e7e9` | `#242424` | dividers and the toolbar edge; decorative only |
| Control border | `--input` | `#8e9194` | `#5f5f5f` | the outline of a text field, at least 3:1 |
| Primary | `--primary` | `#0b1515` | `#f4f4f4` | the one primary button per view (ink, inverse ink in dark) |
| Accent | `--periwinkle` | `#5566dc` | `#a1b5f8` | the main data series, progress, focus ring, one control per screen |
| Accent tint | `--periwinkle-tint` | `#e3e8fd` | `#232843` | fill behind accent text and the selected chip |
| Accent ink | `--periwinkle-ink` | `#3c4ab8` | `#c3cffb` | accent-colored text on ground or tint |
| Coral | `--coral` | `#e8703f` | `#fc885e` | attention: paused, truncated, warnings, a second series |
| Green | `--positive` | `#2f9e4f` | `#9ed49c` | running, live, answering, success |
| Red | `--destructive` | `#d83a34` | `#f0706a` | failures and destructive actions |

Each signal color has a `-tint` (its fill behind text: 12% of the color over the ground in light,
16% in dark) and an `-ink` (its text color: at least 4.5:1 on the ground and on its tint). Charts
use `--chart-1` (periwinkle), `--chart-2` (gray), `--chart-3` (coral), `--chart-4` (green),
`--chart-5` (red) and `--chart-grid`.

Rules:

- One accent, periwinkle, on data: the throughput line, progress fills, the chosen response, the
  focus ring, and one control per screen (the seqs input on a run). The primary button is ink.
- Coral and green are signals, not decoration: each marks a state that the word beside it names.
  Red only on the failing fact.
- One neutral family (zero chroma). The light ink carries a slight teal cast; nothing else does.
- Selection and highlight are neutral (`--accent`), never periwinkle.
- Surfaces are separated by tone, not borders: a card is `--muted` on `--background` with no
  border and no shadow. Hairlines only divide rows and edge the toolbar.
- No gradients except the pixel field. No gradient text, glows or colored shadows.

## Materials

Floating chrome is a frosted, tinted material, in the manner of Android 17's blurred surfaces: a
strong blur behind a mostly opaque fill that carries a faint tint. It is not clear glass and it
does not refract. Tokens and utilities live in `frontend/src/index.css`.

| Utility | Fill | Blur | Used on |
| --- | --- | --- | --- |
| `material-bar` | `--material-tint` at 86% | 30 px, saturate 180% | the toolbar once content scrolls under it, the phone tab bar, sticky section bars and bottom action bars |
| `material-thick` | `--popover` with a quarter of `--material-tint`, at 90% | 40 px, saturate 180% | menus, pop-up lists, popovers, info tips, tooltips, the chart readout, dialogs, alerts, sheets, toasts |
| `material-float` | none | none | a floating panel's 1 px edge (`--material-edge`) and shadow (`--material-shadow`) |

- `--material-tint` is the ground with 4% periwinkle (light) or the shell with 6% periwinkle
  (dark): the faint tonal cast that ties the materials to the accent. A bar that belongs to a
  state sets it to that state's color at the same strength (a toast for a failure: red; the
  preference review bar: periwinkle at 10%).
- The fills keep `--muted-foreground` at 4.5:1 over pure black (light theme) and pure white (dark
  theme) behind the material. Do not lower them.
- Materials go only on chrome that floats over content. Never on cards or rows inside a page.
- The toolbar is transparent at rest (it shows the page). Once the page scrolls by 1 px or more,
  `material-bar` and a 1 px `--border` edge fade in over 200 ms.
- Overlays dim the page with a flat scrim (black 32% light, 56% dark); the dialog on it is
  `material-thick`.
- Fallbacks, all opaque (fill at 100%, no blur): no `backdrop-filter` support, Reduce
  Transparency, Increase Contrast (which also raises `--material-edge` to 3:1). Forced colors: a
  1 px `CanvasText` outline on floating panels.

## Typography

| Family | Stack | Job |
| --- | --- | --- |
| Inter | `'Inter Variable', Inter, ui-sans-serif, system-ui, sans-serif`, optical sizing on | every word of interface text |
| Roboto Mono | `'Roboto Mono Variable', 'Roboto Mono', ui-monospace, Menlo, monospace`, slashed zero | IDs, model names, keys, file names, timestamps, durations in logs, chart axes, data tags |

Both ship with the app (`@fontsource-variable/*`), so every platform renders the same.

Sizes follow the input (Density): pointer-fine is a mouse or trackpad, touch is everything else.

| Role | Family | Pointer-fine | Touch | Weight | Line height | Tracking |
| --- | --- | --- | --- | --- | --- | --- |
| Display (a detail screen's title, the phone large title) | Inter | 32 px | 28 px | 600 | 1.1 | -0.03em |
| Figure (the answer number) | Inter | 28 px | 28 px | 600 | 1.1 | -0.03em, tabular |
| Toolbar title, breadcrumb, section heading (`text-base`) | Inter | 15 px | 17 px (toolbar), 16 px | 600 (parents 500) | 20 px | -0.011em |
| Body, controls (`text-sm`) | Inter | 13 px | 14 px | 400, 500 on buttons | 18 / 20 px | -0.003em |
| Meta, table headers (`text-xs`) | Inter | 12 px | 12 px | 400 | 16 px | 0 |
| Data (`font-mono`) | Roboto Mono | 12 px | 12 px | 400 | 16 px | 0 |
| Data tag and axis (`label-mono`) | Roboto Mono | 12 px | 12 px | 400, uppercase | 16 px | 0.04em |

Rules:

- Sentence case for all interface text. Uppercase only in `label-mono`: chart axis labels, a
  chart's inline flag ("+32% THROUGHPUT"), and data tags beside a value ("FASTEST"). Never as an
  eyebrow above a heading.
- Tabular figures for every number that lines up or changes.
- Text fields are 16 px on touch screens, or iOS zooms the page when one is focused.
- Model output (prompts, thinking, responses) renders as Markdown (GFM, code, KaTeX math) or raw
  with whitespace preserved, by one Rendered/Raw switch remembered per browser. Headings inside
  model output are at most 15 px; text at most 80ch wide.

## Density

| | Pointer-fine | Touch (iOS) |
| --- | --- | --- |
| Buttons, text fields, pop-up buttons | 32 px | 44 pt |
| Small buttons | 28 px | 44 pt |
| Segmented controls | 32 px track, 28 px segments | 44 pt segments |
| Sidebar rows | 32 px | 44 pt |
| Menu and list items | 32 px | 44 pt |
| Table rows | 44 px | 52 px (as stacked rows) |
| Checkboxes, radios | 16 px, invisible 44 pt hit area on touch | same |

`text-sm` and `text-base` read `--text-sm` and `--text-base`, which change under
`@media (pointer: fine)`. The touch rules in `index.css` (44 pt minimums, 16 px fields) are
unlayered so no utility can undercut them.

## Shape: one concentric chain

Every radius derives from the window's. `--window-radius` is the window's real corner radius:

| Where | `--window-radius` | Why |
| --- | --- | --- |
| macOS 26 app | 26 px | the window has an empty unified toolbar (`desktop/src-tauri/src/titlebar.rs`), which gives macOS 26's toolbar-window corners. Measured on macOS 26.6: a title-bar-only window has 16 pt corners; with a unified toolbar, 26 pt. |
| macOS 11 to 15 app | 10 px | those systems' window corners |
| Windows and Linux app | 8 px | Windows 11 corners; the native frame stays |
| Browser | 26 px | no window corners; the design value, so the browser matches the Mac app |

Nested corners are concentric: inner radius = outer radius - the gap between them.

| Element | Radius | Derivation |
| --- | --- | --- |
| Window | `--window-radius` (26) | the system |
| Sidebar panel, side sheets | `--panel-radius` = window - 8 = 18 | floats 8 px inside the window |
| Sidebar rows | 10 | panel 18 - 8 px padding |
| Dialogs and alerts | 18 | same rank as the panel |
| Cards and tiles | 16 | page-level surfaces |
| Wells inside cards (thinking trace, code) | 8 | card 16 - 8 px inset; a well with a 16 px inset is square-cornered (4) |
| Menus, pop-up lists, popovers, toasts | 14 | |
| Menu items | 8 | menu 14 - 6 px padding |
| Buttons, text fields, pop-up buttons | 10 | |
| Segmented track | 10; segments 8 | track 10 - 2 px padding |
| Small buttons (28 px) | 8 | |
| Tooltips | 8 | |
| Chips, status pills, the phone tab bar and its items | capsule | a capsule in a capsule stays concentric |
| Checkboxes | 5 | |

When the window radius is under 20 px (macOS 15 and earlier, Windows, Linux) the sidebar is not
a floating panel: it fills the window's left edge with a 1 px `--border` divider, its corners are
the window's, and rows keep 10 px. Nothing in the page touches a window corner on those systems.

The Tailwind scale in `index.css` is: `rounded-xs` 5, `rounded-sm` 8, `rounded-md` 10,
`rounded-lg` 14, `rounded-xl` 16, `rounded-2xl` 18, `rounded-3xl` 26; `rounded-full` for
capsules. Use the scale or `rounded-(--panel-radius)`; never a raw radius.

Elevation: no shadows on cards. Shadows only on floating material (`material-float`) and on the
raised segment of a segmented control (one 1 px shadow).

## Layout and spacing

- Spacing scale (Tailwind units): 1, 2, 3, 4, 6, 8, 12, 16 (4 to 64 px).
- Shell from 768 px up: the sidebar panel floats 8 px inside the window (a 240 px column: 8 px
  gutter, 224 px panel, 8 px gutter), `--sidebar` fill, `material-float`, `--panel-radius`
  corners, 8 px padding. At its top: the app tile (the mark in white on a 24 px pixel-field tile,
  6 px corners) and the name, in a 32 px row; then Runs, Prompt sets, Endpoints, Preferences;
  Settings at the bottom.
- Sidebar rows: 16 px icon, 8 px gap, label, 8 px side padding, 10 px corners, 32 px tall.
  Labels ink at 500; icons muted until their row is selected. The selected row is `--accent`.
- macOS app: the window has an empty unified toolbar, so macOS centers the traffic lights at
  (26, 26) pt: 19 pt from the left edge, inside the sidebar panel's top band. The top 52 px
  (`--titlebar-height`) of the sidebar is kept free for them, and the sidebar's content starts
  below it. That band and the page toolbar's empty areas move the window (`data-tauri-drag-region`;
  a double click zooms).
- Toolbar: 52 px, sticky, in the page column so its contents line up with the content.
  - Top-level screens (Runs, Prompt sets, Endpoints, Preferences, Settings): the title (the
    page's `h1`, 15 px) then the actions at the end.
  - Detail screens (a run, a prompt set, a preference set, import, new run): a breadcrumb,
    parents in muted ink as links, `/` separators, the current page in ink
    ("Runs / Distill gsm8k"), truncated. The page's `h1` is the display title in the content.
  - Actions: at most one primary (ink) button; the rest secondary; a "…" menu for the rest.
    From 1024 px the actions sit in the bar; below, under the title in the content.
- Detail screen header, in the content, from the top: the display title (up to two lines), then a
  row of chips (status pill first, then the objects it uses: model, endpoint, prompt set, each a
  link), then the facts as tiles. 24 px between the title block and the content.
- Facts as tiles: a row of `--muted` tiles, 16 px corners, 16 px padding, each a figure (28 px)
  over its label (12 px muted). At most four. The first tile may span two columns and hold a
  progress bar. Never repeat a count shown elsewhere on the screen.
- Phones (below 768 px): a 44 px bar under the status-bar inset holds the back button ("‹ Runs")
  or, on the four top-level screens, the Settings gear. The screen starts with the 28 px large
  title. Once it scrolls under the bar, a 17 px title fades in centered in the bar. The four
  destinations sit in a tab bar: a 56 px capsule floating 12 px from the sides and at least 12 px
  above the bottom, `material-bar` with `material-float`; the current tab is a capsule of `--accent`
  with ink text. `--bottom-chrome` keeps the page end clear of it.
- Page padding: 16 px phone, 24 px tablet, 32 px desktop. Page column capped at 80rem; a run is
  96rem.
- Forms: one column, 40rem wide, grouped in `--muted` cards (System Settings style).

## Components

shadcn/ui primitives (Radix), restyled through the tokens above.

- **Buttons** (10 px corners, 32 px, 13 px at 500): `default` ink fill, inverse text, at most one
  per view; `secondary` `--secondary` fill, ink text (toolbar actions, "Load", "Test"); `outline`
  transparent with a 1 px `--input` edge (for a control that must read as one on a busy surface);
  `ghost` no fill until hovered (row actions); `destructive` red ink on red tint.
- **Text fields**: 32 px, 10 px corners, `--background` fill, 1 px `--input` edge (3:1),
  periwinkle focus ring (2 px, 2 px offset, following the corner).
- **Segmented controls** (`ToggleGroup` with `spacing={0}`, the default `TabsList`): a
  `--secondary` track, 2 px padding; the chosen segment raised (`--background` in light,
  `--accent` in dark, 1 px shadow) with ink text at 500, the others muted at 400.
- **Tabs** (`TabsList variant="line"`): for switching views of one list, with counts: 13 px
  labels, the current one ink at 500 with a 2 px ink underline on a full-width hairline; the others
  muted. Counts in muted tabular figures after the label.
- **Chips**: 28 px capsules, `--secondary` fill, 13 px ink; a chip that links shows a hover fill
  of `--accent`. Model names inside chips are mono.
- **Status**: in a header, a status pill: a 24 px capsule in the state's tint, a 6 px dot and the
  word in the state's ink. In lists and tables, the dot and the word only. Normal states
  (Completed, Done, Pending) are plain muted words with no dot. Running and In flight green;
  Paused, Pausing and Truncated coral; Failed red; Thinking periwinkle; Answering green.
- **Tables** (data lists from 1024 px): a header band of `--muted` with 10 px corners and 12 px
  muted labels; 44 px rows divided by hairlines; the row under the pointer `--accent` with 10 px
  corners; the first column ink at 500; numbers right-aligned and tabular; IDs and models mono.
  Below 1024 px, one stacked row per item with its facts on a second line.
- **Cards** (`Panel`): `--muted`, 16 px corners, no border, no shadow; a header row with a 15 px
  title and its actions, 16 px padding. A card is never inside a card; content inside a card uses
  `--background` wells (8 px corners) or plain rows.
- **Menus and lists**: `material-thick` + `material-float`, 14 px corners, 6 px padding, 32 px
  items with 8 px corners; the highlighted item `--accent` with ink text; a checkmark or icon at
  the end; separators full width; destructive items red ink.
- **Dialogs and alerts**: `material-thick` over the scrim, 18 px corners, 20 px padding, title
  15 px at 600; buttons at the end (stacked full width below 640 px), Cancel secondary. Inside
  dialogs, alerts and sheets `--muted` becomes the ground (light) or the raised tone (dark), so a
  card or group stands out from the material instead of matching it.
- **Side sheets**: `material-thick`; edge to edge on phones; from 640 px they float 8 px inside the
  window with `--panel-radius` corners.
- **Tooltips**: `material-thick`, 8 px corners, 12 px text, shown after a 700 ms hover (keyboard
  focus shows them at once). Info tips (the (i) buttons) are popovers in the same material.
- **Progress bar**: 6 px capsule, `--secondary` track, periwinkle fill, red segment for failures,
  always labeled in text beside it.
- **Log rows** (sequences in flight, events): mono time or ID in muted, a state word in its signal
  color (mono, lowercase), then the text in Inter. 32 px rows, no dividers.
- **Plasma tile** (`shared/plasma.tsx`): a square of pixel-field cells from a seed (an endpoint's
  or prompt set's id), 8 px corners at 28 px and 5 px at 20 px. Decorative (`aria-hidden`), always
  next to the name it stands for.

## Data visualization

- Lines 1.5 px; the main series periwinkle, comparison series gray, seqs as a gray step line.
  Fill under the main line a flat 14% tint of its color.
- Axis labels `label-mono` in muted ink; gridlines dashed hairlines; a threshold is a dashed line
  in the series' color with a mono label.
- The readout (hover) is `material-thick`, 8 px corners, mono values.
- A flag on a chart (an annotation) is a capsule in the signal's tint with `label-mono` text. On a
  run, once there are readings on both sides of the last seqs change, a periwinkle flag gives the
  change in tokens per second ("+102% TOK/S").
- A live run's time axis counts back from NOW ("-10M ... NOW"). Paused time is cut out of the axis
  and marked with a dashed line; across days, a session's first label carries the date.
- A run's samples bar (tabs and search) becomes `material-bar` when it sticks under the toolbar and
  shows the run's progress in it.

## Iconography

lucide-react, 16 px, 1.75 px stroke, muted, in navigation and as single affordances only.

## Motion

- Easing `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`. Press 140 ms, hover 180 ms, UI 220 ms,
  overlays 300 ms.
- No decorative motion. Progress bars and charts update in place.
- The toolbar's material and a phone's small title fade in over 200 ms (opacity only).
- Reduced motion: transitions on transform are removed; opacity and color remain.

## Touch and platform (Apple HIG)

- On touch screens every control is at least 44 x 44 pt; checkboxes, radios and switches keep
  their size and grow an invisible hit area.
- Safe-area insets on the phone bar, tab bar and sticky action bars (`viewport-fit=cover`).
- Keyboard: Tab reaches the skip link, the sidebar, the toolbar and then the content; every
  control shows the periwinkle focus ring, following its shape.
- Increase Contrast darkens secondary text, dividers and control edges and makes materials
  opaque; Reduce Transparency makes materials opaque; Reduce Motion removes travel; the theme
  follows the system unless set in Settings.

## Voice

- Plain, exact, short. Buttons say verb and object ("Start run", "Import prompts").
- Every string earns its place: no subtitles that restate a heading, no hints that repeat a
  placeholder or a default, each count shown once.
- Banned: seamless, unlock, supercharge, effortless, magic.

## Do and don't

Do: print zeros as zeros, missing values as a gray dash; show the true ratio on progress; keep an
empty section's frame and say what will appear; make nested corners concentric.

Don't: borders on cards, a card in a card, icons on every card header, sparklines on numbers,
pulsing live dots, a hero on any screen, a colored sidebar, a periwinkle selection, clear or
refracting glass, material on cards or rows inside a page, the pixel field behind text, uppercase
outside `label-mono`.
