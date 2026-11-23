# Store and website assets

Listing copy and images for presenting Stellarator as a finished app: on the Mac App Store, the
Microsoft Store, Linux app stores and the website at `digital.kozugroup.com`. Stellarator is not
published in any store; these files are ready if it is, and most of them serve the website now.

Everything here can be rebuilt after the interface changes: the scripts in `scripts/` capture the
app, frame the captures and finish the PNGs (see [Regenerating the images](#regenerating-the-images)).

## Contents

| Path | What it is for |
| --- | --- |
| `copy/mac-app-store.md` | Every App Store Connect field: name, subtitle, promotional text, description, keywords, What's New, categories, age rating answers, App Privacy, export compliance, review notes |
| `copy/microsoft-store.md` | Every Partner Center listing field: descriptions, up to 20 features, search terms, category, system requirements |
| `copy/linux.md` | Summary and description for Flathub and other AppStream stores, and what a Flathub submission still needs |
| `copy/website.md` | The Stellarator page: head tags, hero, downloads, six feature sections, privacy band, FAQ, system requirements, footer, one- and three-sentence descriptions |
| `copy/privacy.md` | A privacy policy to host on the website |
| `copy/press-kit.md` | Boilerplate, facts, and every image with its size and use |
| `linux/com.thekozugroup.stellarator.metainfo.xml` | AppStream metadata (passes `appstreamcli validate --pedantic`) |
| `images/mac-app-store/` | Six 2880 x 1800 screenshots with headlines, and 1440 x 900 copies |
| `images/microsoft-store/` | Four 1920 x 1080 screenshots with headlines, in a Windows 11 window frame |
| `images/linux/` | Four 1920 x 1080 screenshots of the plain window with a Linux title bar, used by the metainfo file |
| `images/website/` | The hero (2400 x 1350), the social preview (1200 x 630) and six feature images (1600 x 1000) |
| `images/icon/` | The app icon at 1024, 512 and 256 px: macOS shape, Windows and Linux shape, and a full-bleed square |
| `scripts/images.json` | The list of images: file, size, screen, theme, frame, background, headline and subline |
| `scripts/seed.mjs` | Fills an empty data folder with the sample data the screenshots show |
| `scripts/polish-data.py` | Replaces the mock's placeholder answers with prepared ones and tidies the endpoints |
| `scripts/capture.mjs` | Captures the app's screens in headless Chromium |
| `scripts/compose.mjs` | Frames the captures (background, headline, window) and renders the icons |
| `scripts/finish.py` | Flattens, tags as sRGB, writes the 1440 x 900 copies, optimizes and checks the PNGs |
| `scripts/count-copy.py` | Updates the character counts in `copy/` and checks them against each store's limit |

## Images

| File | Pixels | Size |
| --- | --- | ---: |
| `icon/stellarator-1024.png` | 1024 x 1024 | 167 KB |
| `icon/stellarator-256.png` | 256 x 256 | 22 KB |
| `icon/stellarator-512.png` | 512 x 512 | 61 KB |
| `icon/stellarator-macos-1024.png` | 1024 x 1024 | 144 KB |
| `icon/stellarator-macos-256.png` | 256 x 256 | 20 KB |
| `icon/stellarator-macos-512.png` | 512 x 512 | 52 KB |
| `icon/stellarator-square-1024.png` | 1024 x 1024 | 139 KB |
| `linux/01-live-run.png` | 1920 x 1080 | 295 KB |
| `linux/02-import.png` | 1920 x 1080 | 257 KB |
| `linux/03-preference-review.png` | 1920 x 1080 | 292 KB |
| `linux/04-export.png` | 1920 x 1080 | 275 KB |
| `mac-app-store/01-live-run-1440x900.png` | 1440 x 900 | 364 KB |
| `mac-app-store/01-live-run.png` | 2880 x 1800 | 671 KB |
| `mac-app-store/02-endpoints-1440x900.png` | 1440 x 900 | 227 KB |
| `mac-app-store/02-endpoints.png` | 2880 x 1800 | 398 KB |
| `mac-app-store/03-import-1440x900.png` | 1440 x 900 | 320 KB |
| `mac-app-store/03-import.png` | 2880 x 1800 | 623 KB |
| `mac-app-store/04-new-run-1440x900.png` | 1440 x 900 | 236 KB |
| `mac-app-store/04-new-run.png` | 2880 x 1800 | 419 KB |
| `mac-app-store/05-preference-review-1440x900.png` | 1440 x 900 | 379 KB |
| `mac-app-store/05-preference-review.png` | 2880 x 1800 | 745 KB |
| `mac-app-store/06-export-1440x900.png` | 1440 x 900 | 328 KB |
| `mac-app-store/06-export.png` | 2880 x 1800 | 594 KB |
| `microsoft-store/01-live-run.png` | 1920 x 1080 | 276 KB |
| `microsoft-store/02-import.png` | 1920 x 1080 | 259 KB |
| `microsoft-store/03-preference-review.png` | 1920 x 1080 | 290 KB |
| `microsoft-store/04-export.png` | 1920 x 1080 | 258 KB |
| `website/feature-endpoints.png` | 1600 x 1000 | 160 KB |
| `website/feature-export.png` | 1600 x 1000 | 248 KB |
| `website/feature-import.png` | 1600 x 1000 | 261 KB |
| `website/feature-live-run.png` | 1600 x 1000 | 266 KB |
| `website/feature-preference-review.png` | 1600 x 1000 | 316 KB |
| `website/feature-thinking-trace.png` | 1600 x 1000 | 318 KB |
| `website/hero.png` | 2400 x 1350 | 472 KB |
| `website/og-image.png` | 1200 x 630 | 138 KB |

All PNG, sRGB, opaque except the icons. The screenshots are the app's browser build in headless
Chromium, laid out and framed as each platform's window (see below). They show sample data: the
bundled mock model server (models named `mock-…`), the public
`HuggingFaceH4/ultrafeedback_binarized` and `openai/gsm8k` datasets, and answers written for the
screenshots (`scripts/polish-data.py`), not produced by a model.

## Placeholders to fill in

| Where | Placeholder |
| --- | --- |
| `copy/mac-app-store.md`, `copy/microsoft-store.md`, `copy/website.md`, `copy/press-kit.md` | `https://digital.kozugroup.com/stellarator` is a placeholder path for the product page (support and marketing URL) |
| Same files, and `copy/privacy.md` once hosted | `https://digital.kozugroup.com/stellarator/privacy` is a placeholder for the hosted privacy policy |
| `copy/privacy.md` | The contact line at the end |
| `copy/press-kit.md` | The press contact |
| `copy/mac-app-store.md`, review notes | An OpenRouter API key for App Review, and the model id to use with it |
| `linux/...metainfo.xml`, `copy/linux.md` | The homepage is the GitHub repository until the product page exists |

## Regenerating the images

### What it needs

- The repository built: `cargo build -p stellarator-server -p stellarator-mock`, and the app's
  browser build, which the debug server reads from disk: in `app/`,
  `nsh package --target web --out .nsh/web` (the nsh CLI 0.6.1, `curl -fsSL https://nsh.dev/install.sh | sh`;
  on Linux GTK 4.14 or later, see docs/desktop.md).
- Node.js 18 or later with Playwright and its Chromium: `npm install -g playwright` and
  `npx playwright install chromium` (the scripts find a global install by themselves).
- Python 3 with Pillow (`pip install pillow`).
- Internet access, for the Hugging Face import in `seed.mjs` and on the import screen. Without it,
  the import screen falls back to the paste source.

### Steps

From the repository root. `DATA` is any empty folder outside the repository.

```bash
DATA=/tmp/stellarator-store-data && rm -rf "$DATA" && mkdir -p "$DATA"

# 1. The mock model server on port 8000 (the "DGX Spark vLLM" endpoint) and the server, which
#    serves the browser build and the API on port 8096.
target/debug/stellarator-mock --port 8000 --tps 60 &
STELLARATOR_DATA_DIR="$DATA" STELLARATOR_BIND=127.0.0.1:8096 target/debug/stellarator-server &

# 2. Sample data (2 to 4 minutes), then the prepared answers. Stop the server for the second step.
node store/scripts/seed.mjs
kill %2; wait %2; python3 store/scripts/polish-data.py "$DATA"
STELLARATOR_DATA_DIR="$DATA" STELLARATOR_BIND=127.0.0.1:8096 target/debug/stellarator-server &

# 3. Capture (about 5 minutes, most of it the live run), frame, finish.
node store/scripts/capture.mjs
node store/scripts/compose.mjs
python3 store/scripts/finish.py

# 4. Stop everything.
kill %1 %3
```

- `capture.mjs` drives the browser build in headless Chromium and writes the raw screens to
  `store/.work/raw/` (ignored by git) as `<screen>.<theme>.<platform>.png`: 2560 x 1600 for the Mac
  window, 2560 x 1440 for Windows and Linux. Each address carries `?frame=mac`, `windows` or
  `linux`, which makes the app lay the screen out as in that platform's window (on macOS the
  band for the traffic lights). `--only run-new,import` captures some screens only;
  `compose.mjs --only website/` composes some images only.
- The live run resumes the long gsm8k run, runs it at 12 seqs for 150 s and at 32 seqs for 75 s,
  captures, and pauses it again (`LIVE_BEFORE` and `LIVE_AFTER` set the seconds). Each capture uses
  up about 1,600 of its prompts, so seed a fresh folder for each full capture.
- `compose.mjs` also writes the README's screenshots (`docs/assets/screenshot-*.png`, layout
  `readme` in `images.json`): the Mac window at 1.2x over a field of pixels.
- The finished PNGs replace the ones in `images/`. Read every one before committing: look for text
  cut off, a dialog that did not open, or a stale state.

### Changing the words

Headlines and sublines are in `scripts/images.json`: edit, then run `compose.mjs` and `finish.py`
(no new capture needed). After editing anything in `copy/`, run
`python3 store/scripts/count-copy.py` to update the character counts; it fails if a field is over
its store's limit.

### Capturing the desktop app instead

The images show the browser build, which is the same code as the desktop app, laid out as each
platform's window. To show the desktop app itself, take window screenshots without the shadow (on
a Mac, `screencapture -o -l <window id>`) at 1280 x 800 points for the Mac and 1280 x 720 for
Windows and Linux, at 2x, save them in `store/.work/raw/` under the same names, and run
`compose.mjs` and `finish.py`. For the Mac frame, `compose.mjs` draws the window's 26 pt corners
and the traffic lights at (26, 26), (49, 26) and (72, 26) pt; a capture that already includes them
needs the `TRAFFIC` overlay removed. For Windows and Linux it draws the title bar above the
capture, so capture the content area only.
