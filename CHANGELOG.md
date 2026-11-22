# Changelog

Each version's section is copied into its GitHub release under "What's new".

## 0.3.0

### A new look

- A new design system (DESIGN.md): Inter for text and Roboto
  Mono for data, an ink near-black, neutral grays, a periwinkle accent on data, coral for
  attention and green for success, cards set apart by tone instead of borders, large titles on
  detail screens, and a breadcrumb in the toolbar ("Runs / Distill gsm8k").
- Detail screens open with the title, a row of chips (status, model, endpoint, prompt set) and the
  facts as tiles.
- Frosted, tinted materials on the chrome that floats over content: the toolbar once you scroll,
  menus, dialogs, sheets, toasts, the phone tab bar and sticky action bars.
- Endpoints and prompt sets get a small pixel-field tile, the product's new texture.

### Corners that fit the window

- On macOS 26 the window now has the larger corners of macOS 26 apps with a toolbar (26 pt
  instead of 16 pt), and the traffic lights sit where macOS puts them. The sidebar and everything
  inside it are concentric with those corners. On macOS 15 and earlier, Windows and Linux the
  sidebar runs flush with the window's edge.

### New app icon and mark

- A bolder mark and an icon in the new palette.

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
