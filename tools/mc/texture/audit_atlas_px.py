#!/usr/bin/env python3
import io
import json
import sys
import zipfile

from PIL import Image


def load_map(path):
    head = None
    tiles = []
    for line in open(path, encoding="utf-8"):
        parts = line.rstrip("\n").split(" ", 2)
        if parts[0] == "atlas":
            head = [int(x) for x in line.split()[1:6]]
        elif parts[0] == "tile":
            tiles.append((int(parts[1]), parts[2].split(",") if len(parts) > 2 and parts[2] else []))
    return head, tiles


def first_frame(jar, names, path):
    meta = path + ".mcmeta"
    if meta not in names:
        return 0
    try:
        frames = json.loads(jar.read(meta)).get("animation", {}).get("frames")
    except ValueError:
        return 0
    if not frames:
        return 0
    f = frames[0]
    return f["index"] if isinstance(f, dict) else f


def frame0(img, ts, fr):
    w, h = img.size
    if w == 0 or h < w or h % w != 0:
        return None
    px = img.convert("RGBA").load()
    out = bytearray()
    for y in range(ts):
        for x in range(ts):
            out += bytes(px[x * w // ts, y * w // ts + fr * w])
    return bytes(out)


def main():
    jar = zipfile.ZipFile(sys.argv[1])
    atlas = open(sys.argv[2], "rb").read()
    (aw, ah, ts, cols, rows), tiles = load_map(sys.argv[3])
    names = set(jar.namelist())
    checked = bad = fallback = 0
    for tid, cands in tiles:
        hit = None
        for c in cands:
            n = f"assets/minecraft/textures/block/{c}.png"
            if n in names:
                hit = c
                break
        if hit is None:
            fallback += 1
            continue
        path = f"assets/minecraft/textures/block/{hit}.png"
        img = Image.open(io.BytesIO(jar.read(path)))
        want = frame0(img, ts, first_frame(jar, names, path))
        if want is None:
            continue
        x0 = ((tid - 1) % cols) * ts
        y0 = ((tid - 1) // cols) * ts
        got = bytearray()
        for y in range(ts):
            o = ((y0 + y) * aw + x0) * 4
            got += atlas[o:o + ts * 4]
        checked += 1
        if bytes(got) != want:
            diff = sum(1 for i in range(0, len(want), 4) if want[i:i + 4] != bytes(got[i:i + 4]))
            bad += 1
            print(f"tile {tid} {hit}: {diff} of {ts * ts} texels differ")
    print(f"checked {checked} tiles against the jar, {bad} differ, {fallback} without jar texture (default kept)")
    return 1 if bad or checked < 800 else 0


sys.exit(main())
