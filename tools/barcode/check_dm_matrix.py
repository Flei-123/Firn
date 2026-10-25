import random
import subprocess
import sys

import numpy as np
import zxingcpp
from PIL import Image

dmcli, seed, count = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
rng = random.Random(seed)
bad = 0
n = 0


def fail(msg):
    global bad
    bad += 1
    print("FAIL", msg)


def enc(kind, data):
    r = subprocess.run([dmcli, "enc", kind, data.hex() or "00"], capture_output=True, text=True)
    if r.returncode != 0:
        return None
    lines = r.stdout.split("\n")
    rows, cols = map(int, lines[0].split())
    return rows, cols, lines[1:1 + rows]


def dec(rows, cols, lines):
    r = subprocess.run([dmcli, "dec", str(rows), str(cols), "".join(lines)], capture_output=True, text=True)
    p = r.stdout.split()
    if not p or p[0] != "OK":
        return None
    return (b"" if p[1] == "-" else bytes.fromhex(p[1])), int(p[2])


def to_image(rows, cols, lines, scale=6, quiet=2):
    a = np.array([[0 if ch == "1" else 255 for ch in ln] for ln in lines], dtype=np.uint8)
    a = np.pad(a, quiet, constant_values=255)
    return Image.fromarray(np.kron(a, np.ones((scale, scale), dtype=np.uint8)), "L")


def payloads():
    kinds = []
    for _ in range(count):
        ln = rng.choice([1, 2, 3, 5, 8, 12, 20, 40, 100, 300, 800])
        kinds.append(("ascii", "".join(rng.choice("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 -./") for _ in range(ln)).encode()))
        kinds.append(("ascii", "".join(rng.choice("0123456789") for _ in range(ln)).encode()))
        kinds.append(("base256", bytes(rng.randrange(256) for _ in range(min(ln, 400)))))
        kinds.append(("rect", "".join(rng.choice("ABCDEF0123") for _ in range(rng.randint(1, 40))).encode()))
    return kinds


try:
    import pylibdmtx.pylibdmtx as dmtx
except Exception:
    dmtx = None

for kind, data in payloads():
    n += 1
    r = enc(kind, data)
    if r is None:
        if not (kind == "rect"):
            fail(f"encode refused {kind} len {len(data)}")
        continue
    rows, cols, lines = r
    got = dec(rows, cols, lines)
    if got is None or got[0] != data:
        fail(f"self roundtrip {kind} len {len(data)} {rows}x{cols}")
    img = to_image(rows, cols, lines)
    zr = zxingcpp.read_barcodes(img)
    if not any(x.bytes == data for x in zr):
        fail(f"zxing cannot read ours {kind} len {len(data)} {rows}x{cols} -> {[x.bytes[:20] for x in zr]}")
    if dmtx is not None:
        res = dmtx.decode(img, timeout=2000)
        if not any(x.data == data for x in res):
            fail(f"libdmtx cannot read ours {kind} len {len(data)} {rows}x{cols}")
    if got is not None and len(lines) > 12:
        lm = [list(ln) for ln in lines]
        for _ in range(3):
            y, x = rng.randrange(rows), rng.randrange(cols)
            lm[y][x] = "0" if lm[y][x] == "1" else "1"
        g2 = dec(rows, cols, ["".join(l) for l in lm])
        if g2 is None or g2[0] != data:
            fail(f"3 flips not repaired {kind} len {len(data)} {rows}x{cols}")
    for t in range(1):
        pass

fmt = zxingcpp.BarcodeFormat.DataMatrix
for _ in range(count * 3):
    n += 1
    ln = rng.choice([1, 4, 9, 20, 50, 120])
    text = "".join(rng.choice("ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789") for _ in range(ln))
    try:
        bc = zxingcpp.write_barcode(fmt, text, width=0, height=0, quiet_zone=0)
    except TypeError:
        bc = zxingcpp.write_barcode(fmt, text)
    a = np.asarray(bc)
    if a.ndim == 3:
        a = a[:, :, 0]
    rows, cols = a.shape
    lines = ["".join("1" if v < 128 else "0" for v in row) for row in a]
    got = dec(rows, cols, lines)
    if got is None or got[0] != text.encode():
        fail(f"cannot read zxing symbol {rows}x{cols} len {ln}")
print("cases", n, "failures", bad)
sys.exit(1 if bad else 0)
