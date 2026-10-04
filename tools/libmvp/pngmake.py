# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/pngmake.py -- a PNG WRITER of our own for the tests of
# lib/paint/png.fi: every colour type and legal bit depth, Adam7, all five row
# filters, PLTE/tRNS; `expect` computes the RGBA a decoder must give from the
# raw samples (16 bits keep the high octet; 1/2/4-bit grey scaled; colour key
# and palette alpha; an index beyond the palette is black).
import struct, zlib

def chunk(t, d):
    return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xffffffff)

CH = {0: 1, 2: 3, 3: 1, 4: 2, 6: 4}
PASSES = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)]  # x0 y0 dx dy

def pack_row(vals, depth):
    if depth == 8: return bytes(vals)
    if depth == 16: return b"".join(struct.pack(">H", v) for v in vals)
    out = bytearray(); acc = 0; n = 0
    for v in vals:
        acc = (acc << depth) | v; n += depth
        if n == 8: out.append(acc); acc = 0; n = 0
    if n: out.append(acc << (8 - n))
    return bytes(out)

def paeth(a, b, c):
    p = a + b - c; pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    return a if pa <= pb and pa <= pc else (b if pb <= pc else c)

def filt(row, prev, f, bpp):
    out = bytearray(len(row))
    for i, x in enumerate(row):
        a = row[i - bpp] if i >= bpp else 0
        b = prev[i]; c = prev[i - bpp] if i >= bpp else 0
        pr = (0, a, b, (a + b) // 2, paeth(a, b, c))[f]
        out[i] = (x - pr) & 255
    return bytes(out)

def make(rng, ctype, depth, w, h, interlace, plte=None, trns=None, samples=None):
    nch = CH[ctype]; mx = (1 << depth) - 1
    if samples is None:
        lim = len(plte) if ctype == 3 and plte else mx + 1
        samples = [[rng.randrange(lim if ctype == 3 else mx + 1) for _ in range(w * nch)] for _ in range(h)]
    bpp = max(1, nch * depth // 8)
    raw = bytearray()
    passes = PASSES if interlace else [(0, 0, 1, 1)]
    for (x0, y0, dx, dy) in passes:
        xs = list(range(x0, w, dx)); ys = list(range(y0, h, dy))
        if not xs or not ys: continue
        prev = bytes((len(xs) * nch * depth + 7) // 8)
        for y in ys:
            vals = [samples[y][x * nch + c] for x in xs for c in range(nch)]
            row = pack_row(vals, depth)
            f = rng.randrange(5)
            raw.append(f); raw += filt(row, prev, f, bpp); prev = row
    d = chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, depth, ctype, 0, 0, 1 if interlace else 0))
    if plte is not None: d += chunk(b"PLTE", bytes(v for c in plte for v in c))
    if trns is not None: d += chunk(b"tRNS", trns)
    # IDAT in two pieces now and then
    z = zlib.compress(bytes(raw), rng.choice([1, 6, 9]))
    if len(z) > 20 and rng.random() < 0.5:
        k = rng.randrange(1, len(z)); d += chunk(b"IDAT", z[:k]) + chunk(b"IDAT", z[k:])
    else:
        d += chunk(b"IDAT", z)
    return b"\x89PNG\r\n\x1a\n" + d + chunk(b"IEND", b""), samples

def expect(ctype, depth, w, h, samples, plte, trns):
    mx = (1 << depth) - 1
    def to8(v): return v >> 8 if depth == 16 else (v if depth == 8 else v * 255 // mx)
    out = bytearray()
    key = None
    if ctype == 0 and trns and len(trns) >= 2: key = (struct.unpack(">H", trns[:2])[0],)
    if ctype == 2 and trns and len(trns) >= 6: key = struct.unpack(">HHH", trns[:6])
    for y in range(h):
        for x in range(w):
            if ctype == 0:
                v = samples[y][x]; g = to8(v); px = (g, g, g, 0 if key and v == key[0] else 255)
            elif ctype == 2:
                v = tuple(samples[y][x * 3 + k] for k in range(3)); px = (to8(v[0]), to8(v[1]), to8(v[2]), 0 if key and v == key else 255)
            elif ctype == 3:
                i = samples[y][x]; c = plte[i] if i < len(plte) else (0, 0, 0)
                px = (c[0], c[1], c[2], trns[i] if trns and i < len(trns) else 255)
            elif ctype == 4:
                g = to8(samples[y][x * 2]); px = (g, g, g, to8(samples[y][x * 2 + 1]))
            else:
                px = tuple(to8(samples[y][x * 4 + k]) for k in range(4))
            out += bytes(px)
    return bytes(out)

