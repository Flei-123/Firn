#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_gif.py <gif_probe> <workdir> [more.gif ...] -- lib/gif
# against Pillow (libgif's role): the same RGBA octets for the first frame,
# the same canvases for every later frame, the same delays and loop count,
# on a generated corpus (every palette size, interlaced, transparent,
# partial frames, offsets, disposal 1/2/3, local palettes, big pictures,
# incompressible noise) plus a set of GIFs written by hand with an LZW
# encoder of our own (no initial clear, no EOI, one-octet sub-blocks,
# repeated-pattern codes, a table that fills up, min code size 2..8,
# indices outside the colour table) and the files given as arguments.
#
# WHERE PILLOW AND THIS DECODER DIFFER ON PURPOSE (not compared):
#   * the colour of fully transparent pixels in frames after the first
#     (Pillow keeps whatever the canvas had) -- compared as "transparent";
#   * disposal 2 of a frame without a transparent index: Pillow fills the
#     background colour index, browsers (and this decoder) clear to
#     transparent; disposal 3 on the FIRST frame: Pillow does nothing,
#     browsers clear -- the corpus avoids both;
#   * Pillow carries a disposal method over to frames whose own is 0;
#   * a GIF whose first frame has no transparent index becomes an RGB
#     canvas in Pillow, so a later frame cannot make pixels transparent
#     there (the corpus gives such files a transparent index in frame 0).
import os, random, struct, subprocess, sys
try:
    from PIL import Image, ImageDraw
except ImportError:
    print("  skip: Pillow not installed"); sys.exit(0)
probe, work, extra = sys.argv[1], sys.argv[2], sys.argv[3:]
rng = random.Random(3)

def shapes(w, h, mode="RGB", n=40):
    im = Image.new(mode, (w, h), (0,) * len(mode))
    d = ImageDraw.Draw(im)
    for _ in range(n):
        x0, x1 = sorted([rng.randint(0, w - 1), rng.randint(0, w - 1)])
        y0, y1 = sorted([rng.randint(0, h - 1), rng.randint(0, h - 1)])
        d.ellipse([x0, y0, x1, y1], fill=tuple(rng.randint(0, 255) for _ in mode))
    return im

files = []
def save(name, im, **kw):
    p = os.path.join(work, name)
    im.save(p, **kw)
    files.append(p)

base = shapes(61, 47)
for nc in (2, 3, 4, 5, 16, 17, 64, 128, 255, 256):
    save("q%d.gif" % nc, base.quantize(nc))
save("inter.gif", base.quantize(16), interlace=True)
save("s1x1.gif", shapes(1, 1).quantize(2))
for w, h in ((2, 2), (7, 3), (3, 9), (8, 8), (17, 5), (1, 40), (40, 1)):
    save("s%dx%d.gif" % (w, h), shapes(w, h).quantize(8))
    save("si%dx%d.gif" % (w, h), shapes(w, h).quantize(8), interlace=True)
big = shapes(400, 300).quantize(256)
save("big.gif", big); save("big_i.gif", big, interlace=True)
noise = Image.frombytes("L", (200, 200), bytes(rng.randint(0, 255) for _ in range(40000)))
save("noise.gif", noise); save("noise_i.gif", noise, interlace=True)
t = base.quantize(32)
save("transp.gif", t, transparency=5); save("transp0.gif", t, transparency=0)
fr = [shapes(50, 40).quantize(16) for _ in range(6)]
save("anim.gif", fr[0], save_all=True, append_images=fr[1:], duration=[50, 100, 70, 20, 0, 300], loop=0)
save("anim_loop3.gif", fr[0], save_all=True, append_images=fr[1:], duration=100, loop=3)
save("disp1.gif", fr[0], save_all=True, append_images=fr[1:], duration=100, disposal=1)
rf = []
for k in range(6):
    im = Image.new("RGBA", (48, 32), (0, 0, 0, 0)); d = ImageDraw.Draw(im)
    for _ in range(3):
        x0, x1 = sorted([rng.randint(0, 47), rng.randint(0, 47)]); y0, y1 = sorted([rng.randint(0, 31), rng.randint(0, 31)])
        d.rectangle([x0, y0, x1, y1], fill=(rng.randint(0, 255), rng.randint(0, 255), rng.randint(0, 255), 255))
    rf.append(im)
save("rgba_d1.gif", rf[0], save_all=True, append_images=rf[1:], duration=80, loop=0, disposal=1)
save("rgba_d2.gif", rf[0], save_all=True, append_images=rf[1:], duration=80, loop=0, disposal=2)
save("rgba_d3.gif", rf[0], save_all=True, append_images=rf[1:], duration=80, loop=0, disposal=[1, 3, 3, 3, 3, 3])
save("rgba_mix.gif", rf[0], save_all=True, append_images=rf[1:], duration=80, loop=0, disposal=[1, 2, 3, 1, 2, 3])

# ------------------------------------------------ GIFs written by hand
def lzw(indices, minbits, initial_clear=True, eoi=True, max_codes=4096):
    clear = 1 << minbits
    end = clear + 1
    out = bytearray(); acc = 0; nacc = 0
    def put(code, size):
        nonlocal acc, nacc
        acc |= code << nacc; nacc += size
        while nacc >= 8:
            out.append(acc & 255); acc >>= 8; nacc -= 8
    size = minbits + 1
    table = {bytes([i]): i for i in range(clear)}
    nxt = end + 1
    if initial_clear:
        put(clear, size)
    cur = b""
    for v in indices:
        nb = cur + bytes([v])
        if nb in table:
            cur = nb
            continue
        put(table[cur], size)
        if nxt < max_codes:
            table[nb] = nxt; nxt += 1
            if nxt == (1 << size) + 1 and size < 12:   # the code that no longer fits
                size += 1
        else:
            put(clear, size)       # table full: clear and start over
            table = {bytes([i]): i for i in range(clear)}
            nxt = end + 1; size = minbits + 1
        cur = bytes([v])
    if cur:
        put(table[cur], size)
    if eoi:
        put(end, size)
    if nacc:
        out.append(acc & 255)
    return bytes(out)

def blocks(data, bs):
    out = bytearray()
    for i in range(0, len(data), bs):
        c = data[i:i + bs]; out.append(len(c)); out += c
    out.append(0)
    return bytes(out)

def gif(w, h, pal, frames, loops=None, trailer=True, bg=0):
    """frames: list of dicts(x,y,w,h,indices,minbits,pal,interlace,transp,delay,disposal,bs,clear,eoi)"""
    nbits = max(1, (len(pal) - 1).bit_length()) if pal else 0
    out = bytearray(b"GIF89a" + struct.pack("<HH", w, h))
    out.append((0x80 | ((nbits - 1) & 7) | 0x70) if pal else 0x70)
    out += bytes([bg, 0])
    if pal:
        full = list(pal) + [(0, 0, 0)] * ((1 << nbits) - len(pal))
        for c in full: out += bytes(c)
    if loops is not None:
        out += b"\x21\xff\x0bNETSCAPE2.0\x03\x01" + struct.pack("<H", loops) + b"\x00"
    for f in frames:
        if f.get("transp") is not None or f.get("delay") is not None or f.get("disposal"):
            pk = (f.get("disposal", 0) << 2) | (1 if f.get("transp") is not None else 0)
            out += b"\x21\xf9\x04" + bytes([pk]) + struct.pack("<H", f.get("delay", 0)) + bytes([f.get("transp") or 0]) + b"\x00"
        lp = f.get("pal")
        ln = max(1, (len(lp) - 1).bit_length()) if lp else 0
        pkf = (0x80 | (ln - 1) if lp else 0) | (0x40 if f.get("interlace") else 0)
        out += b"\x2c" + struct.pack("<HHHH", f.get("x", 0), f.get("y", 0), f["w"], f["h"]) + bytes([pkf])
        if lp:
            full = list(lp) + [(0, 0, 0)] * ((1 << ln) - len(lp))
            for c in full: out += bytes(c)
        mb = f["minbits"]
        out += bytes([mb]) + blocks(lzw(f["indices"], mb, f.get("clear", True), f.get("eoi", True)), f.get("bs", 255))
    if trailer:
        out.append(0x3b)
    return bytes(out)

def interlaced(rows, w, h):
    order = list(range(0, h, 8)) + list(range(4, h, 8)) + list(range(2, h, 4)) + list(range(1, h, 2))
    return [v for y in order for v in rows[y * w:(y + 1) * w]]

def hand(name, data):
    p = os.path.join(work, name)
    open(p, "wb").write(data); files.append(p)

def pal_n(n):
    return [((i * 37) % 256, (i * 91 + 20) % 256, (i * 53 + 7) % 256) for i in range(n)]

w, h = 23, 17
for mb in range(2, 9):
    n = 1 << mb
    idx = [rng.randrange(n) for _ in range(w * h)]
    hand("hand_mb%d.gif" % mb, gif(w, h, pal_n(n), [dict(w=w, h=h, indices=idx, minbits=mb)]))
idx = [rng.randrange(4) for _ in range(w * h)]
hand("hand_noclear.gif", gif(w, h, pal_n(4), [dict(w=w, h=h, indices=idx, minbits=2, clear=False)]))
hand("hand_noeoi.gif", gif(w, h, pal_n(4), [dict(w=w, h=h, indices=idx, minbits=2, eoi=False)]))
hand("hand_notrailer.gif", gif(w, h, pal_n(4), [dict(w=w, h=h, indices=idx, minbits=2)], trailer=False))
for bs in (1, 2, 3, 7, 255):
    hand("hand_bs%d.gif" % bs, gif(w, h, pal_n(4), [dict(w=w, h=h, indices=idx, minbits=2, bs=bs)]))
hand("hand_same.gif", gif(40, 30, pal_n(4), [dict(w=40, h=30, indices=[1] * 1200, minbits=2)]))      # KwKwK
hand("hand_same2.gif", gif(41, 50, pal_n(256), [dict(w=41, h=50, indices=[200] * 2050, minbits=8)]))
hand("hand_alt.gif", gif(40, 30, pal_n(4), [dict(w=40, h=30, indices=[i % 2 for i in range(1200)], minbits=2)]))
long = [(i // 7) % 4 for i in range(120 * 90)]
hand("hand_fill.gif", gif(120, 90, pal_n(4), [dict(w=120, h=90, indices=[rng.randrange(256) for _ in range(120 * 90)], minbits=8)]))
hand("hand_runs.gif", gif(120, 90, pal_n(4), [dict(w=120, h=90, indices=long, minbits=2)]))
rows = [rng.randrange(8) for _ in range(w * h)]
hand("hand_inter.gif", gif(w, h, pal_n(8), [dict(w=w, h=h, indices=interlaced(rows, w, h), minbits=3, interlace=True)]))
hand("hand_oob.gif", gif(w, h, pal_n(5), [dict(w=w, h=h, indices=[rng.randrange(8) for _ in range(w * h)], minbits=3)]))
hand("hand_nogct.gif", gif(w, h, None, [dict(w=w, h=h, indices=[rng.randrange(8) for _ in range(w * h)], minbits=3, pal=pal_n(8))]))
hand("hand_localpal.gif", gif(w, h, pal_n(4), [
    dict(w=w, h=h, indices=idx, minbits=2, delay=7),
    dict(w=w, h=h, indices=idx[::-1], minbits=2, pal=pal_n(16)[8:12], delay=9)], loops=5))
hand("hand_part.gif", gif(30, 20, pal_n(8), [
    dict(w=30, h=20, indices=[rng.randrange(8) for _ in range(600)], minbits=3, delay=3, transp=7),
    dict(x=5, y=4, w=10, h=6, indices=[rng.randrange(8) for _ in range(60)], minbits=3, delay=4, transp=2, disposal=1),
    dict(x=12, y=9, w=9, h=7, indices=[rng.randrange(8) for _ in range(63)], minbits=3, delay=5, transp=1, disposal=2),
    dict(x=0, y=0, w=30, h=20, indices=[rng.randrange(8) for _ in range(600)], minbits=3, delay=6, transp=0, disposal=1)], loops=0))
hand("hand_trail.gif", gif(w, h, pal_n(4), [dict(w=w, h=h, indices=idx, minbits=2)]) + b"junk after the trailer")
hand("hand_comment.gif", gif(w, h, pal_n(4), [dict(w=w, h=h, indices=idx, minbits=2)])[:13 + 12] + b"\x21\xfe\x05hello\x00" + gif(w, h, pal_n(4), [dict(w=w, h=h, indices=idx, minbits=2)])[13 + 12:])

files += [f for f in extra if os.path.exists(f)]

def norm(b):
    b = bytearray(b)
    for i in range(0, len(b), 4):
        if b[i + 3] == 0: b[i] = b[i + 1] = b[i + 2] = 0
    return bytes(b)

bad = 0; frames_total = 0; pixels = 0
out = os.path.join(work, "out.raw")
for f in files:
    try:
        im = Image.open(f); nf = getattr(im, "n_frames", 1)
        refs = []
        for i in range(nf):
            im.seek(i); refs.append((im.convert("RGBA").tobytes(), im.info.get("duration", 0)))
        loop = im.info.get("loop", None)
        size = im.size
    except Exception as e:
        print("  note: Pillow refuses %s (%s)" % (os.path.basename(f), type(e).__name__))
        continue
    r = subprocess.run([probe, f, out, "anim"], capture_output=True, text=True)
    if r.stdout.strip() != "OK":
        bad += 1; print("  FAIL %s: %s" % (os.path.basename(f), r.stdout.strip())); continue
    raw = open(out, "rb").read()
    nl = raw.index(b"\n"); w_, h_, c, lp = map(int, raw[:nl].split()); raw = raw[nl + 1:]
    dl = []
    for i in range(c):
        nl = raw.index(b"\n"); dl.append(int(raw[:nl].split()[1])); raw = raw[nl + 1:]
    msgs = []
    if c != nf: msgs.append("%d frames, Pillow %d" % (c, nf))
    if (w_, h_) != size: msgs.append("size %dx%d, Pillow %dx%d" % (w_, h_, size[0], size[1]))
    else:
        for i in range(min(c, nf)):
            mine = raw[i * w_ * h_ * 4:(i + 1) * w_ * h_ * 4]; ref, dur = refs[i]
            if (mine != ref) if i == 0 else (norm(mine) != norm(ref)):
                msgs.append("frame %d differs in %d octets" % (i, sum(1 for a, b in zip(mine, ref) if a != b)))
            if dl[i] != dur: msgs.append("delay %d: %d, Pillow %d" % (i, dl[i], dur))
        # the loop count: Pillow has no key when the extension is absent
        if loop is not None and lp != loop: msgs.append("loops %d, Pillow %d" % (lp, loop))
        if loop is None and nf > 1 and lp != 1: msgs.append("loops %d without the extension" % lp)
    if msgs:
        bad += 1; print("  DIFF %s: %s" % (os.path.basename(f), "; ".join(msgs[:3])))
    frames_total += c; pixels += w_ * h_ * c
print("gif: %d files (%d frames, %d pixels) decoded octet for octet like Pillow %s, %d differ"
      % (len(files), frames_total, pixels, Image.__version__, bad))
sys.exit(1 if bad else 0)
