# Design system

## Overview

An Operate tool, used in short glances during long runs. Quiet neutral chrome; color goes on data.
The accent is coil copper, taken from the copper windings of a stellarator's magnet coils, and it
marks the main data series (throughput, progress) and at most one control per screen. Light and dark
themes follow the system setting; both are designed, neither is an inversion. Ground and grays are
true neutral (zero chroma) so the copper reads as the only hue.

Identity elements:

- The mark: a twisted ring drawn for this product, in the sidebar with the name.
- The signature element: on a run, throughput (tokens per second) drawn over the seqs setting as a
  step line, so the effect of changing concurrency is visible. Below it, the sequences in flight,
  each showing whether it is thinking or answering.
- Trade words in labels: seqs, thinking trace, truncated, chosen, rejected.

Platform: the layout follows macOS 26 and iOS 26 (Apple HIG): a floating sidebar, a unified
toolbar per screen, a floating tab bar on phones, macOS control shapes and density on mouse and
trackpad, iOS metrics on touch. Where macOS 26 draws Liquid Glass, Stellarator draws a frosted
material in the manner of Android 16's blurred surfaces, opaque enough to read over anything
(Materials). The same UI runs in the macOS app, in a browser on any desktop, and on phones.

## Color

All values are OKLCH. Tokens live in `frontend/src/index.css`.

| Role | Token | Light | Dark | Use |
| --- | --- | --- | --- | --- |
| Ground | `--background` | 0.99 0 0 | 0.165 0 0 | most of the surface |
| Shell | `--sidebar` | 0.965 0 0 | 0.2 0 0 | the sidebar panel's tint, one step off the ground |
| Surface | `--muted` | 0.965 0 0 | 0.215 0 0 | panels, thinking trace, code wells |
| Ink | `--foreground` | 0.2 0 0 | 0.93 0 0 | text and strong marks |
| Muted ink | `--muted-foreground` | 0.48 0 0 | 0.7 0 0 | secondary text, at least 4.5:1 |
| Border | `--border` | 0.91 0 0 | 0.28 0 0 | dividers, card edges |
| Control border | `--input` | 0.62 0 0 | 0.5 0 0 | outlines of text fields, at least 3:1 |
| Raised control | `--control` | 1 0 0 | 0.3 0 0 | push and pop-up buttons, the chosen segment, toolbar items |
| Control hover | `--control-hover` | 0.965 0 0 | 0.34 0 0 | a raised control under the pointer or open |
| Control edge | `--control-edge` | 0 0 0 / 0.14 | 1 0 0 / 0.1 | hairline around a raised control |
| Segment track | `--segment-track` | 0 0 0 / 0.06 | 1 0 0 / 0.08 | the capsule behind segmented controls |
| Selection | `--sidebar-accent` | 0.905 0 0 | 0.3 0 0 | the selected sidebar row |
| Highlight | `--accent` | 0.925 0 0 | 0.31 0 0 | the highlighted menu, list and pop-up item |
| Primary | `--primary` | 0.2 0 0 | 0.93 0 0 | the one primary button per view (near-black, near-white in dark) |
| Accent (copper) | `--copper` | 0.56 0.15 45 | 0.72 0.14 52 | main data series, progress fill, one control |
| Focus | `--ring` | 0.56 0.15 45 | 0.72 0.14 52 | focus rings |
| Negative | `--destructive` | 0.55 0.2 27 | 0.68 0.18 25 | failures, destructive actions |
| Positive | `--positive` | 0.52 0.12 150 | 0.72 0.13 150 | completed |
| Warning | `--warning` | 0.6 0.13 75 | 0.78 0.13 80 | truncated, paused |

Status is a word. Normal states (Done, Completed, Pending) are plain gray text; live or exceptional
states (Running, Paused, Failed, Truncated, Thinking, Answering) add a 6 px dot at full strength
before the word. Running is green so copper stays on data.

Rules:

- One accent, copper, on data: the progress fill, the throughput line, the chosen response, and
  one control (the seqs input). The primary button is near-black.
- One neutral family: true neutral, chroma 0.
- Selection and highlight are neutral (`--sidebar-accent`, `--accent`), never copper.
- No gradients. The only fill under a line is a flat 14% tint.
- Red only on the failing fact: the failed count, the failed row, the error message.

## Materials

Where macOS 26 uses Liquid Glass, Stellarator uses a frosted material: a strong blur behind a
mostly opaque tonal fill, in the manner of Android 16's blurred surfaces. It is not clear glass and
it does not refract. Tokens and utilities live in `frontend/src/index.css`.

| Utility | Fill | Blur | Saturation | Used on |
| --- | --- | --- | --- | --- |
| `material-bar` | `--material-tint` (default `--background`) at 85% | 32 px | 160% | the toolbar once content scrolls under it, the sidebar panel (tint `--sidebar`), the phone tab bar |
| `material-thick` | `--popover` at 90% | 40 px | 160% | menus, pop-up lists, popovers, info tips, tooltips, the chart readout, dialogs, alerts, sheets |
| `material-float` | none | none | none | the edge and shadow of a floating panel: a 1 px `--material-edge` hairline (black 10% light, white 12% dark) and `--material-shadow` (0 1 2 and 0 8 24 -4, black 6% and 14% light, 30% and 50% dark) |

Rules:

- The fills are the lowest that keep `--muted-foreground` text at 4.5:1 over any content: pure
  black under the material in the light theme, pure white under it in the dark theme. Do not lower
  them.
- Materials go only on chrome that floats over content (the list above). Never on panels, cards or
  table rows inside a page.
- The toolbar is transparent at rest (it shows the page color). When the page scrolls by 1 px or
  more, `material-bar` and a 1 px `--material-edge` divider fade in over 200 ms.
- Overlays still dim the page with a flat scrim (black 40% light, 60% dark); the dialog on top of
  it is `material-thick`.
- Fallbacks, all opaque (fill at 100%, no blur): a browser without `backdrop-filter`
  (`@supports`), Reduce Transparency (`prefers-reduced-transparency: reduce`), Increase Contrast
  (`prefers-contrast: more`, which also sets `--material-edge` and `--control-edge` to the
  `--input` strength, 3:1). Forced colors (Windows High Contrast): system colors, and floating
  panels get a 1 px `CanvasText` outline.
- `-webkit-backdrop-filter` is set with `backdrop-filter` (WKWebView, Safari).

## Typography

UI text uses the system font on Apple platforms and IBM Plex Sans elsewhere:
`-apple-system, BlinkMacSystemFont, 'IBM Plex Sans', ui-sans-serif, system-ui, sans-serif`. Safari,
the macOS app and Chrome or Firefox on a Mac get SF Pro; Windows, Linux and Android get Plex Sans.
IBM Plex Mono is unchanged on every platform.

Sizes follow the input (Density): pointer-fine is a mouse or trackpad, touch is everything else.

| Role | Family | Pointer-fine | Touch | Weight | Line height |
| --- | --- | --- | --- | --- | --- |
| Toolbar title (page title, from 768 px) | UI | 15 px | 17 px | 600 | 20 px |
| Large title (page title, phones) | UI | 28 px | 28 px | 600 | 1.25, tracking -0.02em |
| Section heading (`text-base`) | UI | 15 px | 16 px | 600 | 20 px / 1.5 |
| Body, controls (`text-sm`) | UI | 13 px | 14 px | 400 | 18 px / 20 px |
| Meta, table headers, tab labels (`text-xs`) | UI | 12 px | 12 px | 400 | 1.4 |
| Answer figure | UI | 30 px | 30 px | 500 | 1.1, tabular |
| IDs, model names, keys, file names | IBM Plex Mono | 13 px | 13 px | 400 | 1.5 |

Rules:

- Sentence case everywhere. No uppercase tracked labels.
- Tabular figures for every number that lines up or changes.
- Text fields are 16 px on touch screens, or iOS zooms the page when one is focused.
- Model output (prompts, thinking, responses) renders as Markdown (GFM, code, KaTeX math) or raw
  with whitespace preserved, by one Rendered/Raw switch remembered per browser. Headings inside
  model output are at most 16 px; text at most 80ch wide.

## Density

| | Pointer-fine (macOS) | Touch (iOS) |
| --- | --- | --- |
| Push buttons, text fields, pop-up buttons | 28 px tall | 44 pt |
| Small buttons | 24 px | 44 pt |
| Segmented controls | 28 px track, 24 px segments | 44 pt segments |
| Sidebar rows | 28 px | 44 pt |
| Menu and list items | 26 px (4 px padding, 18 px line) | 44 pt |
| Checkboxes, radios | 16 px with an invisible 44 pt hit area on touch | same |

`text-sm` and `text-base` read `--text-sm` and `--text-base`, which change under
`@media (pointer: fine)`; components add `pointer-fine:h-7` and the like. The touch rules in
`index.css` (44 pt minimums, 16 px fields) are unlayered so no utility can undercut them.

## Layout and spacing

- Spacing scale (Tailwind units): 1, 2, 3, 4, 6, 8, 12, 16 (4 to 64 px).
- Shell from 768 px up: the sidebar is a panel floating 8 px inside the window (a 240 px column:
  8 px gutter, 224 px panel, 8 px gutter), `material-bar` tinted `--sidebar`, `material-float`,
  18 px corners, 8 px padding. At its top, the mark and name in a 28 px row centered on the
  toolbar's band (26 px from the top); then Runs, Prompt sets, Endpoints, Preferences; Settings at
  the bottom.
- Sidebar rows: 16 px icon, 8 px gap, label, 8 px side padding, 10 px corners. Labels are
  `--foreground`; icons `--muted-foreground` until their row is selected. The selected row is
  `--sidebar-accent`, same weight as the others.
- Toolbar: every screen's title and actions live in a toolbar at the top of the content column
  (Page in `shared/page.tsx`), 52 px tall from 768 px up, sticky, with its contents in the page
  column so the title lines up with the content. Order: back button (when the screen has a
  parent), title (the page's only `h1`, one line, truncated with the full text in its tooltip),
  then the actions at the end. From 1024 px up the actions are toolbar buttons in the bar; from
  768 to 1023 px they sit under the toolbar at the top of the content, beside the meta line.
- Below the toolbar: the meta line (status, model, counts) in 13 px muted text, then 24 px, then
  the content. Content starts 16 px below the toolbar.
- Phones (below 768 px): a 44 px bar under the status-bar inset holds the back button ("‹ Runs") or,
  on the four top-level screens, the Settings gear at the leading end. The screen starts with a
  28 px large title, its meta line and its actions. Once the large title scrolls under the bar, a
  17 px title fades in centered in the bar. The four destinations sit in a tab bar: a 56 px capsule
  floating 12 px from the sides and at least 12 px above the bottom (clear of the home indicator),
  `material-bar` with `material-float`; the current tab is a neutral capsule (`--foreground` at
  8%) with ink text, the others gray. Content scrolls under the bar and the tab bar;
  `--bottom-chrome` keeps the page end and sticky bottom bars clear of the tab bar.
- macOS app: no separate title bar. The toolbar band (52 px) is also the title bar: the traffic
  lights sit in it at the top left of the sidebar panel, 20 pt from the window's left edge and
  centered 26 pt from the top (`TRAFFIC_LIGHTS` in `desktop/src-tauri/src/titlebar.rs`, with
  `--titlebar-height` in `index.css`: change both together). The sidebar's content starts below the
  band. The band over the sidebar and the toolbar's empty areas move the window; no control sits
  under the traffic lights (docs/desktop.md).
- `--top-chrome` (toolbar plus status-bar inset) is the offset for sticky headers inside pages and
  the page's `scroll-padding-top`.
- Page padding: 16 px phone, 24 px tablet, 32 px desktop. Page column capped at 80rem.
- Forms: one column, 40rem wide.

## Shape and elevation

- Radius scale (`index.css`): 4, 6, 8, 12, 16, 18 and 24 px (`rounded-sm` to `rounded-4xl`).
- Push buttons, text fields and pop-up buttons: 8 px. Segmented controls, toolbar buttons and the
  phone tab bar: capsules. Menus, pop-up lists, popovers and panels: 12 px. Dialogs, alerts, the
  sidebar panel and side sheets: 18 px.
- Nested corners are concentric: inner radius = outer radius - padding. A 12 px menu with 4 px
  padding holds 8 px items; the 18 px sidebar panel with 8 px padding holds 10 px rows; a capsule
  track holds capsule segments.
- Panels: 1 px border, no shadow. Shadows only on floating material (`material-float`) and on
  raised controls (one 1 px shadow, `shadow-xs`).

## Components

shadcn/ui primitives (Radix), restyled through the tokens above.

- Buttons: `default` (near-black, at most once per view, chosen by state), `outline` (the macOS
  push button: `--control` fill, `--control-edge` hairline, 1 px shadow), `ghost` (no fill until
  hovered), `destructive` (red text on a 10% red tint). In the toolbar every button is a capsule
  and `ghost` buttons take the `outline` look.
- Segmented controls: `ToggleGroup` with `spacing={0}` and the default `TabsList`. A
  `--segment-track` capsule with 2 px padding; the chosen segment is raised (`--control`, hairline,
  1 px shadow) with ink text at weight 500, the others gray at 400. A track that wraps to two rows
  keeps 16 px corners (24 px on touch).
- Pop-up buttons (`SelectTrigger`): a push button with an up-and-down chevron at the end; the list
  opens over the trigger with the chosen item checked.
- Text fields: 8 px corners, `--input` border (3:1), copper focus ring.
- Menus and lists: `material-thick`, 12 px corners, 4 px padding; the highlighted item is
  `--accent`; destructive items are red text.
- Dialogs and alerts: `material-thick` over the flat scrim, 18 px corners, 20 px padding, title
  `text-base` at 600; buttons at the end with no footer band (stacked full width below 640 px).
- Side sheets: `material-thick`; edge to edge on phones, floating 8 px inside the window with 18 px
  corners from 640 px up.
- Tooltips: `material-thick`, 6 px corners, 12 px text, no arrow, 4 px from the trigger. Info tips
  (the (i) buttons) are popovers in the same material.
- Status: see Color.

## Iconography

lucide-react, 16 px, 1.5 px stroke, gray, in navigation and as single affordances only.

## Motion

- Easing `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`. Press 140 ms, hover 180 ms, UI 220 ms,
  overlays 300 ms.
- No decorative motion. Progress bars and charts update in place; nothing redraws from zero.
- The toolbar's material, and a phone's small title, fade in over 200 ms (opacity only).
- Reduced motion: transitions on transform are removed; opacity and color remain.

## Touch and platform (Apple HIG)

- On touch screens every control is at least 44 x 44 pt; checkboxes, radios and switches keep
  their size and grow an invisible hit area.
- Safe-area insets on the phone bar, tab bar and sticky action bars (`viewport-fit=cover`).
- Back: on phones a chevron plus the parent screen's title ("‹ Runs"), as iOS; from 768 px up a
  chevron capsule toolbar button named "Back to Runs", with that name in its tooltip.
- Keyboard: Tab reaches the skip link, the sidebar, the toolbar (back, actions) and then the
  content, in that order; every control shows the copper focus ring, following its shape.
- Increase Contrast darkens secondary text, dividers and control edges and makes materials opaque;
  Reduce Transparency makes materials opaque; Reduce Motion removes travel; theme follows the
  system unless set in Settings.

## Voice

- Plain, exact, short. Buttons say verb and object ("Start run", "Import prompts"); a link that
  opens the run form says "New run".
- Every string earns its place (squint): no subtitles that restate a heading, no hints that repeat
  a placeholder or a default, each count shown once.
- Banned: seamless, unlock, supercharge, effortless, magic.

## Do and don't

Do: print zeros as zeros, missing values as a gray dash; show the true ratio on progress; keep an
empty section's frame and say what will appear.

Don't: KPI tiles with icons, sparklines on numbers, pulsing live dots, a hero on any screen, colored
sidebars, a copper selection, clear or refracting glass, material on cards or panels inside a page.
