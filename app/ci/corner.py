"""Measures a window's corner from a PNG screenshot with transparency (macOS `screencapture -o -l`):
for the top rows, how many pixels at the left end are fully transparent (alpha 0) and how many are
partly transparent. The number of rows that start with transparent pixels is about the corner
radius in pixels (points times the screen's scale). Only the standard library.

    python3 app/ci/corner.py window.png [rows]
"""

import struct
import sys
import zlib


def read_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    pos, idat, header = 8, b"", None
    while pos < len(data):
        (length,) = struct.unpack(">I", data[pos:pos + 4])
        kind = data[pos + 4:pos + 8]
        body = data[pos + 8:pos + 8 + length]
        if kind == b"IHDR":
            header = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
        pos += 12 + length
    width, height, depth, color, _, _, interlace = header
    assert depth == 8 and interlace == 0, f"unsupported PNG: depth {depth}, interlace {interlace}"
    channels = {6: 4, 2: 3, 0: 1, 4: 2}[color]
    raw = zlib.decompress(idat)
    stride = width * channels
    rows, prev = [], bytearray(stride)
    for y in range(height):
        start = y * (stride + 1)
        kind, line = raw[start], bytearray(raw[start + 1:start + 1 + stride])
        for i in range(stride):
            a = line[i - channels] if i >= channels else 0
            b = prev[i]
            c = prev[i - channels] if i >= channels else 0
            if kind == 1:
                line[i] = (line[i] + a) & 255
            elif kind == 2:
                line[i] = (line[i] + b) & 255
            elif kind == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif kind == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(line)
        prev = line
    return width, height, channels, rows


def main(path, count):
    width, height, channels, rows = read_png(path)
    print(f"{path}: {width} x {height}, {channels} channels")
    if channels != 4:
        print("no alpha channel: cannot measure the corner")
        return
    corner_rows = 0
    for y in range(min(count, height)):
        line = rows[y]
        clear = partial = 0
        for x in range(width):
            alpha = line[x * 4 + 3]
            if alpha == 0 and partial == 0:
                clear += 1
            elif alpha < 255:
                partial += 1
            else:
                break
        if clear:
            corner_rows = y + 1
        print(f"row {y:2d}: transparent {clear:3d}  partly transparent {partial:3d}")
    print(f"rows starting with transparent pixels: {corner_rows} (about the corner radius in pixels)")


if __name__ == "__main__":
    main(sys.argv[1], int(sys.argv[2]) if len(sys.argv) > 2 else 40)
