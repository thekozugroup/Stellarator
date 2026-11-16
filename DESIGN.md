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

Status pills are a pale tint with dark text of the same hue (`--*-tint` / `--*-ink` pairs).

Rules:

- One accent, copper, on data. The primary button is near-black.
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
- Model output (prompts, thinking, responses) is shown raw with whitespace preserved, in the sans
  at 14 px, at most 80ch wide.

## Layout and spacing

- Spacing scale (Tailwind units): 1, 2, 3, 4, 6, 8, 12, 16 (4 to 64 px).
- Shell: left sidebar 224 px from 768 px up; below that a top bar with a menu sheet.
- Page padding: 16 px phone, 24 px tablet, 32 px desktop. Page column capped at 80rem.
- Forms: one column, 40rem wide.

## Shape and elevation

- Radius: 6 px controls, 8 px panels and dialogs, full for pills. Nested radii concentric.
- Panels: 1 px border, no shadow. Shadows only on menus, popovers, dialogs and toasts.

## Components

shadcn/ui primitives (Radix), restyled through the tokens above. Buttons: `default` (near-black,
once per view), `outline`, `ghost`, `destructive`. Status: `StatusPill` with a word and a tint,
never color alone.

## Iconography

lucide-react, 16 px, 1.5 px stroke, gray, in navigation and as single affordances only.

## Motion

- Easing `--ease-out: cubic-bezier(0.23, 1, 0.32, 1)`. Press 140 ms, hover 180 ms, UI 220 ms,
  overlays 300 ms.
- No decorative motion. Progress bars and charts update in place; nothing redraws from zero.
- Reduced motion: transitions on transform are removed; opacity and color remain.

## Voice

- Plain, exact, short. Buttons say verb and object ("Start run", "Import prompts").
- Banned: seamless, unlock, supercharge, effortless, magic.

## Do and don't

Do: print zeros as zeros, missing values as a gray dash; show the true ratio on progress; keep an
empty section's frame and say what will appear.

Don't: KPI tiles with icons, sparklines on numbers, pulsing live dots, a hero on any screen, colored
sidebars.
