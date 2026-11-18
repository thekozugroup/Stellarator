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

## Color

All values are OKLCH. Tokens live in `frontend/src/index.css`.

| Role | Token | Light | Dark | Use |
| --- | --- | --- | --- | --- |
| Ground | `--background` | 0.99 0 0 | 0.165 0 0 | most of the surface |
| Shell | `--sidebar` | 0.97 0 0 | 0.19 0 0 | sidebar, one step off the ground |
| Surface | `--muted` | 0.965 0 0 | 0.215 0 0 | panels, thinking trace, code wells |
| Ink | `--foreground` | 0.2 0 0 | 0.93 0 0 | text and strong marks |
| Muted ink | `--muted-foreground` | 0.48 0 0 | 0.7 0 0 | secondary text, at least 4.5:1 |
| Border | `--border` | 0.91 0 0 | 0.28 0 0 | dividers, card edges |
| Control border | `--input` | 0.62 0 0 | 0.5 0 0 | outlines of fields and controls, at least 3:1 |
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
- No gradients. The only fill under a line is a flat 14% tint.
- Red only on the failing fact: the failed count, the failed row, the error message.

## Typography

| Role | Family | Size | Weight | Line height |
| --- | --- | --- | --- | --- |
| Page title | IBM Plex Sans | 24 px | 600 | 1.2 |
| Section heading | IBM Plex Sans | 16 px | 600 | 1.4 |
| Body, controls | IBM Plex Sans | 14 px | 400 | 1.5 |
| Meta, table headers | IBM Plex Sans | 12 px | 400 | 1.4 |
| Answer figure | IBM Plex Sans | 30 px | 500 | 1.1, tabular |
| IDs, model names, keys, file names | IBM Plex Mono | 13 px | 400 | 1.5 |

Rules:

- Sentence case everywhere. No uppercase tracked labels.
- Tabular figures for every number that lines up or changes.
- Model output (prompts, thinking, responses) renders as Markdown (GFM, code, KaTeX math) or raw
  with whitespace preserved, by one Rendered/Raw switch remembered per browser. Headings inside
  model output are at most 16 px; text at most 80ch wide.

## Layout and spacing

- Spacing scale (Tailwind units): 1, 2, 3, 4, 6, 8, 12, 16 (4 to 64 px).
- Shell: left sidebar 224 px from 768 px up. Below that, a top bar (mark, Settings) and a bottom
  tab bar for Runs, Prompt sets, Endpoints and Preferences, above the home-indicator inset.
- macOS app: no separate title bar. A 40 px strip across the top moves the window; the window
  controls sit on the sidebar in it, and nothing interactive does (docs/desktop.md).
- Page padding: 16 px phone, 24 px tablet, 32 px desktop. Page column capped at 80rem.
- Forms: one column, 40rem wide.

## Shape and elevation

- Radius: `--radius` 6 px for controls; panels and dialogs 8 px (`rounded-xl`). Nested radii
  concentric.
- Panels: 1 px border, no shadow. Shadows only on menus, popovers, dialogs and toasts.

## Components

shadcn/ui primitives (Radix), restyled through the tokens above. Buttons: `default` (near-black,
at most once per view, chosen by state), `outline`, `ghost`, `destructive`. Overlays dim the page
with a flat scrim, no blur. Status: see Color.

## Iconography

lucide-react, 16 px, 1.5 px stroke, gray, in navigation and as single affordances only.

## Motion

- Easing `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`. Press 140 ms, hover 180 ms, UI 220 ms,
  overlays 300 ms.
- No decorative motion. Progress bars and charts update in place; nothing redraws from zero.
- Reduced motion: transitions on transform are removed; opacity and color remain.

## Touch and platform (Apple HIG)

- On touch screens every control is at least 44 x 44 pt; checkboxes, radios and switches keep
  their size and grow an invisible hit area.
- Safe-area insets on the top bar, tab bar and sticky action bars (`viewport-fit=cover`).
- Back links are a chevron plus the parent screen's title.
- Increase Contrast darkens secondary text, dividers and control edges; Reduce Motion removes
  travel; theme follows the system unless set in Settings.

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
sidebars.
