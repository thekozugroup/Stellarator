"""Finishes the images compose.mjs wrote: flattens them to opaque RGB (the icons keep their
transparency), tags them as sRGB, writes the smaller copies listed under "copies" in images.json,
optimizes every PNG and checks each file's pixel size and weight.

    python3 store/scripts/finish.py [--images store/images]

Prints a Markdown table of every image (path, pixels, size) for store/README.md.
Needs Python 3 with Pillow.
"""

import json
import sys
from pathlib import Path

from PIL import Image, PngImagePlugin

HERE = Path(__file__).resolve().parent
MAX_BYTES = 4 * 1024 * 1024


def srgb_info() -> PngImagePlugin.PngInfo:
    info = PngImagePlugin.PngInfo()
    info.add(b"sRGB", b"\x00")  # rendering intent: perceptual
    return info


def save(img: Image.Image, path: Path) -> None:
    img.save(path, optimize=True, pnginfo=srgb_info())


def finish(images_dir: Path) -> list[tuple[str, tuple[int, int], int]]:
    spec = json.loads((HERE / "images.json").read_text())
    rows = []
    problems = []
    for item in spec["images"]:
        path = images_dir / item["file"]
        if not path.exists():
            problems.append(f"missing: {item['file']}")
            continue
        img = Image.open(path)
        if img.mode != "RGB":
            background = Image.new("RGB", img.size, (255, 255, 255))
            background.paste(img.convert("RGBA"), mask=img.convert("RGBA").getchannel("A"))
            img = background
        if list(img.size) != item["size"]:
            problems.append(f"{item['file']}: {img.size}, expected {tuple(item['size'])}")
        save(img, path)
        rows.append((item["file"], img.size, path.stat().st_size))
        for w, h in item.get("copies", []):
            copy = path.with_name(f"{path.stem}-{w}x{h}{path.suffix}")
            save(img.resize((w, h), Image.LANCZOS), copy)
            rows.append((str(copy.relative_to(images_dir)), (w, h), copy.stat().st_size))
    for icon in spec.get("icons", []):
        path = images_dir / icon["file"]
        if not path.exists():
            problems.append(f"missing: {icon['file']}")
            continue
        img = Image.open(path)
        img = img.convert("RGB") if icon.get("opaque") else img.convert("RGBA")
        save(img, path)
        rows.append((icon["file"], img.size, path.stat().st_size))
    for name, _, size in rows:
        if size > MAX_BYTES:
            problems.append(f"{name}: {size / 1048576:.1f} MB, over {MAX_BYTES // 1048576} MB")
    if problems:
        print("\n".join(problems), file=sys.stderr)
        sys.exit(1)
    return rows


if __name__ == "__main__":
    images = Path(sys.argv[sys.argv.index("--images") + 1]) if "--images" in sys.argv else HERE.parent / "images"
    rows = finish(images)
    print("| File | Pixels | Size |\n| --- | --- | ---: |")
    for name, (w, h), size in sorted(rows):
        print(f"| `{name}` | {w} x {h} | {size / 1024:,.0f} KB |")
    print(f"\n{len(rows)} images, {sum(r[2] for r in rows) / 1048576:.1f} MB in all")
