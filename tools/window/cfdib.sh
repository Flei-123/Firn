#!/bin/bash
# SPDX-License-Identifier: MPL-2.0
# tools/window/cfdib.sh -- round clip: CF_DIBV5 / CF_DIB <-> PNG (lib/window/cfdib.fi).
#   PNG -> CF_DIBV5: every header field and the BGRA rows checked against an independent Python build.
#   CF_DIB/CF_DIBV5 of other programs (24 bit padded rows, top-down, 8 bit palette, 32 bit BI_RGB with and without alpha,
#   BI_BITFIELDS with 40 octet header, V5) -> PNG: pixels checked through PIL. Damaged / unsupported blocks must fail.
Q="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"; T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT; cd "$Q" || exit 2
FIRNLIB="$Q/lib" "${FIRNC:-$Q/compiler/target/release/firnc}" --no-cache --opt-level=dev-fast -o "$T/cfdib" lib/window/cfdib_main.fi >/dev/null 2>"$T/b.err" || { tail -8 "$T/b.err"; echo "FAIL cfdib (build)"; exit 1; }
python3 -I - "$T/cfdib" <<'PY'
import subprocess, sys, struct, zlib, random, io
from PIL import Image
exe = sys.argv[1]
bad = 0
def check(name, ok, extra=""):
    global bad
    print(("ok   " if ok else "FAIL ") + name + (" " + extra if extra else "")); bad += 0 if ok else 1
def run(mode, data):
    r = subprocess.run([exe], input=mode + data, capture_output=True)
    return r.returncode, r.stdout
def mkpng(w, h, px):  # px: list of (r,g,b,a) top-down
    raw = b"".join(b"\x00" + b"".join(bytes(px[y*w+x]) for x in range(w)) for y in range(h))
    def ch(t, d): return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
    return b"\x89PNG\r\n\x1a\n" + ch(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)) + ch(b"IDAT", zlib.compress(raw)) + ch(b"IEND", b"")
def topix(png):
    im = Image.open(io.BytesIO(png)).convert("RGBA"); return im.size, list(im.getdata())
rnd = random.Random(5)
# ---- A: PNG -> CF_DIBV5
for (w, h) in [(1, 1), (37, 23), (300, 200)]:
    px = [(rnd.randrange(256), rnd.randrange(256), rnd.randrange(256), rnd.randrange(256)) for _ in range(w*h)]
    rc, dib = run(b"P", mkpng(w, h, px))
    exp_hdr = struct.pack("<IiiHHIIiiIIIII", 124, w, h, 1, 32, 3, w*h*4, 3780, 3780, 0, 0, 0xFF0000, 0xFF00, 0xFF) + struct.pack("<I", 0xFF000000) + struct.pack("<I", 0x73524742) + b"\0"*36 + struct.pack("<IIIIIII", 0, 0, 0, 4, 0, 0, 0)
    body = b"".join(bytes((px[y*w+x][2], px[y*w+x][1], px[y*w+x][0], px[y*w+x][3])) for y in range(h-1, -1, -1) for x in range(w))
    check("A png->dibv5 %dx%d" % (w, h), rc == 0 and dib == exp_hdr + body, "len %d want %d" % (len(dib), len(exp_hdr + body)))
    # ---- C: round trip
    rc2, png2 = run(b"D", dib)
    check("C round trip %dx%d (alpha kept)" % (w, h), rc2 == 0 and topix(png2) == ((w, h), px))
# ---- B: foreign DIBs
W, H = 5, 3
base = [(rnd.randrange(256), rnd.randrange(256), rnd.randrange(256)) for _ in range(W*H)]
def hdr40(w, h, bpp, comp, ncol=0): return struct.pack("<IiiHHIIiiII", 40, w, h, 1, bpp, comp, 0, 2835, 2835, ncol, 0)
def rows(px, w, h, bpp, topdown=False):
    out = []
    for y in range(h):
        r = b""
        for x in range(w):
            R, G, B = px[y*w+x][:3]
            r += bytes((B, G, R)) if bpp == 24 else bytes((B, G, R, px[y*w+x][3] if len(px[y*w+x]) > 3 else 0))
        r += b"\0" * ((-len(r)) % 4); out.append(r)
    return b"".join(out if topdown else out[::-1])
def want(px, alpha=None): return [(p[0], p[1], p[2], 255 if alpha is None else alpha[i]) for i, p in enumerate(px)]
cases = []
cases.append(("24 bit bottom-up, padded rows", hdr40(W, H, 24, 0) + rows(base, W, H, 24), want(base)))
cases.append(("24 bit top-down", hdr40(W, -H, 24, 0) + rows(base, W, H, 24, True), want(base)))
a4 = [(p[0], p[1], p[2], rnd.randrange(1, 256)) for p in base]
cases.append(("32 bit BI_RGB, alpha octets used", hdr40(W, H, 32, 0) + rows(a4, W, H, 32), a4))
a0 = [(p[0], p[1], p[2], 0) for p in base]
cases.append(("32 bit BI_RGB, all alpha 0 -> opaque", hdr40(W, H, 32, 0) + rows(a0, W, H, 32), want(base)))
# 40 octet header + BI_BITFIELDS, masks R,G,B swapped order (RGBA in memory), no alpha mask
sw = b"".join(bytes((p[0], p[1], p[2], 0)) for p in base[::-1][:0])
rowsb = b""
for y in range(H-1, -1, -1):
    for x in range(W):
        R, G, B = base[y*W+x]; rowsb += bytes((R, G, B, 0))
cases.append(("32 bit BI_BITFIELDS, 40 octet header, RGB masks", hdr40(W, H, 32, 3) + struct.pack("<III", 0xFF, 0xFF00, 0xFF0000) + rowsb, want(base)))
# 8 bit palette
pal = [(rnd.randrange(256), rnd.randrange(256), rnd.randrange(256)) for _ in range(7)]
idx = [rnd.randrange(7) for _ in range(W*H)]
prow = b"".join((bytes(idx[y*W:(y+1)*W]) + b"\0" * ((-W) % 4)) for y in range(H-1, -1, -1))
palb = b"".join(bytes((c[2], c[1], c[0], 0)) for c in pal)
cases.append(("8 bit palette", hdr40(W, H, 8, 0, 7) + palb + prow, [(pal[i][0], pal[i][1], pal[i][2], 255) for i in idx]))
for name, blk, exp in cases:
    rc, png = run(b"D", blk + b"\0\0")  # GlobalSize padding must not matter
    ok = rc == 0 and topix(png) == ((W, H), exp)
    check("B " + name, ok, "" if ok else "rc=%d" % rc)
# ---- D: must fail
for name, blk in [("16 bit", hdr40(W, H, 16, 0) + b"\0" * 64), ("RLE8", hdr40(W, H, 8, 1, 2) + b"\0" * 64),
                  ("truncated pixels", hdr40(W, H, 24, 0) + b"\0" * 10), ("OS/2 12 octet header", struct.pack("<IHHHH", 12, W, H, 1, 24) + b"\0" * 64),
                  ("empty", b"")]:
    rc, _ = run(b"D", blk)
    check("D refuse " + name, rc != 0 and rc != -11)
rc, _ = run(b"P", b"not a png at all")
check("D refuse garbage png", rc != 0 and rc != -11)
sys.exit(1 if bad else 0)
PY
rc=$?; [ $rc = 0 ] && echo "PASS cfdib" || echo "FAIL cfdib"; exit $rc
