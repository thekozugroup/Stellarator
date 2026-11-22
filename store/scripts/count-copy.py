"""Updates the character counts in the store copy and checks them against the stores' limits.

A heading that ends in "(N of LIMIT characters)" counts the ```text block under it; one that ends in
"(longest N of LIMIT characters each)" counts every line of the block and reports the longest. N may
be any number or the word COUNT; it is rewritten in place. Run it after editing the copy:

    python3 store/scripts/count-copy.py

Exits with an error when a field is over its limit.
"""

import re
import sys
from pathlib import Path

COPY = Path(__file__).resolve().parent.parent / "copy"
HEADING = re.compile(r"^(#+ .*\()(?:longest )?(\d[\d,]*|COUNT)( of )([\d,]+)( characters)( each)?(\)\s*)$")


def block_after(lines: list[str], start: int) -> list[str] | None:
    """The lines of the first ```text block after `start`, before the next heading."""
    i = start + 1
    while i < len(lines) and not lines[i].startswith("#"):
        if lines[i].strip() == "```text":
            j = i + 1
            while j < len(lines) and lines[j].strip() != "```":
                j += 1
            return lines[i + 1 : j]
        i += 1
    return None


def main() -> None:
    over = []
    for path in sorted(COPY.glob("*.md")):
        lines = path.read_text().split("\n")
        changed = False
        for n, line in enumerate(lines):
            m = HEADING.match(line)
            if not m:
                continue
            block = block_after(lines, n)
            if block is None:
                over.append(f"{path.name}: no text block under '{line}'")
                continue
            limit = int(m.group(4).replace(",", ""))
            each = m.group(6) is not None
            if each:
                items = [b for b in block if b.strip()]
                count = max(len(b) for b in items)
                for b in items:
                    if len(b) > limit:
                        over.append(f"{path.name}: {len(b)} > {limit}: {b[:60]}...")
            else:
                count = len("\n".join(block))
                if count > limit:
                    over.append(f"{path.name}: '{line.strip()}' has {count} characters, limit {limit}")
            new = f"{m.group(1)}{'longest ' if each else ''}{count:,}{m.group(3)}{m.group(4)}{m.group(5)}{m.group(6) or ''}{m.group(7)}"
            if new != line:
                lines[n] = new
                changed = True
            print(f"{path.name}: {line.split('(')[0].strip('# ').strip()}: {count} of {limit}")
        if changed:
            path.write_text("\n".join(lines))
    if over:
        print("\n".join(over), file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
