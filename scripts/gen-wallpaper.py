#!/usr/bin/env python3
"""
Generate procedural Pancake DE wallpapers — zero dependencies, pure Python.
Outputs PPM files (supported by swaybg, eog, and most image tools).
Usage: python3 gen-wallpaper.py [--width W] [--height H] [--out DIR] [--count N]
"""
import argparse, math, random, os, struct, sys, zlib

def make_aero_wallpaper(w: int, h: int, seed: int) -> list[list[tuple[int,int,int]]]:
    rng = random.Random(seed)
    base = (9, 15, 36)

    orbs = [
        (0.25 + rng.uniform(-0.15, 0.15), 0.60 + rng.uniform(-0.15, 0.15),
         0.38 + rng.uniform(-0.08, 0.08), 56,  132, 255, 0.55),
        (0.75 + rng.uniform(-0.15, 0.15), 0.35 + rng.uniform(-0.15, 0.15),
         0.32 + rng.uniform(-0.08, 0.08), 20,  174, 210, 0.40),
        (0.50 + rng.uniform(-0.10, 0.10), 0.15 + rng.uniform(-0.08, 0.08),
         0.20 + rng.uniform(-0.05, 0.05), 108,  72, 224, 0.32),
        (rng.uniform(0.1, 0.9), rng.uniform(0.1, 0.9),
         0.14 + rng.uniform(-0.03, 0.06), 240, 168,  48, 0.18),
    ]

    rows = []
    for y in range(h):
        row = []
        yf = y / h
        for x in range(w):
            xf = x / w
            r, g, b = base
            for (cx_f, cy_f, rad_f, or_, og, ob, a) in orbs:
                rad = rad_f  # in normalised coords
                dist = math.sqrt((xf - cx_f) ** 2 + (yf - cy_f) ** 2)
                inf = max(0.0, 1.0 - dist / rad) ** 2 * a
                r = min(255, int(r + or_ * inf))
                g = min(255, int(g + og * inf))
                b = min(255, int(b + ob * inf))
            row.append((r, g, b))
        rows.append(row)
    return rows


def write_png(rows: list, path: str):
    h = len(rows)
    w = len(rows[0])

    def chunk(tag: bytes, data: bytes) -> bytes:
        c = tag + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)

    # IHDR
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)

    # IDAT
    raw = b""
    for row in rows:
        raw += b"\x00"  # filter type None
        for (r, g, b) in row:
            raw += bytes([r, g, b])
    compressed = zlib.compress(raw, 6)

    data = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", ihdr)
        + chunk(b"IDAT", compressed)
        + chunk(b"IEND", b"")
    )
    with open(path, "wb") as f:
        f.write(data)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--width",  type=int, default=1920)
    ap.add_argument("--height", type=int, default=1080)
    ap.add_argument("--out",    default=".")
    ap.add_argument("--count",  type=int, default=3)
    args = ap.parse_args()

    os.makedirs(args.out, exist_ok=True)
    for i in range(args.count):
        seed = 42 + i * 17
        rows = make_aero_wallpaper(args.width, args.height, seed)
        path = os.path.join(args.out, f"pancake-wallpaper-{i+1:02d}.png")
        write_png(rows, path)
        print(f"  {path}")

if __name__ == "__main__":
    main()
