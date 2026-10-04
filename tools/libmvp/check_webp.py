#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_webp.py <webp_probe> <workdir> [more.webp | dir ...]
# -- lib/webp against Pillow (libwebp): the same RGBA octets, lossless AND
# lossy, no tolerance. Pillow writes a corpus (lossless, lossy at many
# qualities and methods, alpha raw/compressed/exact, palette, sizes 1x1 ..
# 333x211, ICC/EXIF/XMP chunks, animations); animations with every
# combination of frame offset, blend and dispose flag are muxed by hand
# from Pillow-encoded stills and compared frame by frame with Pillow's
# WebPAnimDecoder (delays and loop count too). Files and directories given
# as arguments (the Modrinth icons, FleiLauncher's images) are checked as
# they are.
import glob, os, random, struct, subprocess, sys
try:
    from PIL import Image, ImageDraw
except ImportError:
    print("  skip: Pillow not installed"); sys.exit(0)
probe, work, extra = sys.argv[1], sys.argv[2], sys.argv[3:]
rng = random.Random(5)

def shapes(w, h, mode="RGB", n=40):
    im = Image.new(mode, (w, h), (0,) * len(mode))
    d = ImageDraw.Draw(im)
    for _ in range(n):
        x0, x1 = sorted([rng.randint(0, w - 1), rng.randint(0, w - 1)])
        y0, y1 = sorted([rng.randint(0, h - 1), rng.randint(0, h - 1)])
        d.ellipse([x0, y0, x1, y1], fill=tuple(rng.randint(0, 255) for _ in mode))
    for x in range(w):
        for y in range(0, h, 7):
            im.putpixel((x, y), tuple((x * (k + 2) + y * (k + 1)) % 256 for k in range(len(mode))))
    return im

files = []
def save(name, im, **kw):
    p = os.path.join(work, name)
    im.save(p, **kw)
    files.append(p)

sizes = ((1, 1), (2, 2), (3, 17), (17, 3), (16, 16), (33, 1), (1, 33), (100, 37), (255, 130), (333, 211))
for w, h in sizes:
    rgb = shapes(w, h); rgba = shapes(w, h, "RGBA")
    save("ll_%dx%d.webp" % (w, h), rgb, lossless=True)
    save("lla_%dx%d.webp" % (w, h), rgba, lossless=True)
    save("ly_%dx%d.webp" % (w, h), rgb, quality=75)
    save("lya_%dx%d.webp" % (w, h), rgba, quality=75)
rgb = shapes(120, 80); rgba = shapes(120, 80, "RGBA")
for q in (0, 2, 10, 30, 50, 75, 90, 100):
    save("q%d.webp" % q, rgb, quality=q)
    save("qa%d.webp" % q, rgba, quality=q)
for m in range(7):
    save("m%d.webp" % m, rgb, quality=60, method=m)
    save("ma%d.webp" % m, rgba, quality=60, method=m, alpha_quality=50)
    save("llm%d.webp" % m, rgba, lossless=True, quality=70, method=m)
for aq in (0, 20, 60, 100):
    save("alphaq%d.webp" % aq, rgba, quality=70, alpha_quality=aq)
save("exact.webp", rgba, quality=70, exact=True)
save("ll_exact.webp", rgba, lossless=True, exact=True)
save("pal5.webp", rgb.quantize(5), lossless=True)
save("pal200.webp", rgb.quantize(200), lossless=True)
save("pal2.webp", rgb.quantize(2), lossless=True)
save("grey.webp", rgb.convert("L"), lossless=True)
save("greyy.webp", rgb.convert("L"), quality=80)
save("icc.webp", rgb, quality=80, icc_profile=b"x" * 301, exif=b"Exif\0\0" + b"y" * 33, xmp=b"<x/>")
flat = Image.new("RGBA", (200, 150), (10, 120, 200, 255)); flat.putpixel((5, 5), (255, 0, 0, 255))
save("flat.webp", flat, quality=50); save("flat_ll.webp", flat, lossless=True)
grad = Image.new("RGB", (256, 64))
for x in range(256):
    for y in range(64): grad.putpixel((x, y), (x, y * 4 % 256, 255 - x))
save("grad.webp", grad, quality=40); save("grad_ll.webp", grad, lossless=True)
noise = Image.frombytes("RGB", (97, 61), bytes(rng.randint(0, 255) for _ in range(97 * 61 * 3)))
save("noise.webp", noise, quality=90); save("noise_ll.webp", noise, lossless=True)
big = shapes(700, 500, "RGBA", 120)
save("big.webp", big, quality=80); save("big_ll.webp", big, lossless=True)

# animations written by Pillow
fr = [shapes(48, 32, "RGBA", 10) for _ in range(6)]
save("anim_ll.webp", fr[0], save_all=True, append_images=fr[1:], duration=[40, 80, 120, 40, 40, 200], loop=3, lossless=True)
save("anim_ly.webp", fr[0], save_all=True, append_images=fr[1:], duration=100, loop=0, quality=70)
save("anim_mixed.webp", fr[0], save_all=True, append_images=fr[1:], duration=100, minimize_size=True, allow_mixed=True, quality=60)

# animations muxed by hand: every offset / blend / dispose combination
def chunks_of(path):
    d = open(path, "rb").read()[12:]
    out = []; i = 0
    while i + 8 <= len(d):
        sz = struct.unpack("<I", d[i + 4:i + 8])[0]
        out.append((d[i:i + 4], d[i + 8:i + 8 + sz])); i += 8 + sz + (sz & 1)
    return out
def chunk(tag, body):
    return tag + struct.pack("<I", len(body)) + body + (b"\0" if len(body) & 1 else b"")
def mux(path, W, H, frames, loops=0, bg=0):
    body = chunk(b"VP8X", bytes([0x12, 0, 0, 0]) + (W - 1).to_bytes(3, "little") + (H - 1).to_bytes(3, "little"))
    body += chunk(b"ANIM", struct.pack("<IH", bg, loops))
    for (x, y, w, h, dur, blend, dispose, sub) in frames:
        hdr = (x // 2).to_bytes(3, "little") + (y // 2).to_bytes(3, "little") + (w - 1).to_bytes(3, "little") + (h - 1).to_bytes(3, "little") + dur.to_bytes(3, "little") + bytes([(0 if blend else 2) | (1 if dispose else 0)])
        body += chunk(b"ANMF", hdr + b"".join(chunk(t, b) for t, b in sub))
    open(path, "wb").write(b"RIFF" + struct.pack("<I", len(body) + 4) + b"WEBP" + body)
    files.append(path)
stills = []
for k in range(8):
    w, h = rng.randint(1, 40), rng.randint(1, 30)
    kind = k % 4
    im = shapes(w, h, "RGBA" if kind != 2 else "RGB", 6)
    p = os.path.join(work, "still%d.webp" % k)
    if kind == 0: im.save(p, lossless=True)
    elif kind == 1: im.save(p, quality=70)
    elif kind == 2: im.save(p, quality=70)
    else:
        im.putalpha(Image.frombytes("L", (w, h), bytes(rng.choice([0, 0, 255, 128, 77]) for _ in range(w * h)))); im.save(p, lossless=True)
    stills.append((w, h, [(t, b) for t, b in chunks_of(p) if t in (b"ALPH", b"VP8 ", b"VP8L")]))
for case in range(40):
    W, H = rng.randint(20, 80), rng.randint(16, 60)
    fl = []
    for i in range(rng.randint(1, 7)):
        w, h, sub = rng.choice(stills)
        if rng.random() < 0.25:                # a full-size frame
            sub2 = rng.choice([s for s in stills]); w, h, sub = sub2
            W, H = max(W, w), max(H, h)
        x = rng.randrange(0, max(1, W - w + 1) // 2) * 2 if W - w >= 2 else 0
        y = rng.randrange(0, max(1, H - h + 1) // 2) * 2 if H - h >= 2 else 0
        if x + w > W or y + h > H: W, H = max(W, x + w), max(H, y + h)
        fl.append((x, y, w, h, rng.choice([0, 10, 50, 250]), rng.random() < 0.6, rng.random() < 0.4, sub))
    # a frame that covers the canvas now and then
    if case % 5 == 0:
        w, h, sub = stills[0]; fl.insert(rng.randint(0, len(fl)), (0, 0, w, h, 30, True, False, sub))
        W, H = max(W, w), max(H, h)
    mux(os.path.join(work, "mux%02d.webp" % case), W, H, fl, loops=rng.choice([0, 1, 5]))

for e in extra:
    if os.path.isdir(e):
        for f in sorted(glob.glob(os.path.join(e, "*"))):
            try:
                if open(f, "rb").read(12)[8:12] == b"WEBP": files.append(f)
            except OSError: pass
    elif os.path.exists(e):
        files.append(e)

bad = 0; pixels = 0; frames_total = 0; checked = 0
out = os.path.join(work, "out.raw")
for f in files:
    try:
        im = Image.open(f); nf = getattr(im, "n_frames", 1)
        refs = []
        for i in range(nf):
            im.seek(i); refs.append((im.convert("RGBA").tobytes(), im.info.get("duration", 0)))
        loop = im.info.get("loop", None); size = im.size
    except Exception as e:
        print("  note: Pillow refuses %s (%s)" % (os.path.basename(f), type(e).__name__))
        continue
    checked += 1
    msgs = []
    # the still API: the first frame
    r = subprocess.run([probe, f, out], capture_output=True, text=True)
    if r.stdout.strip() != "OK":
        msgs.append("still: " + r.stdout.strip())
    else:
        raw = open(out, "rb").read(); hdr, px = raw.split(b"\n", 1); w_, h_ = map(int, hdr.split())
        if (w_, h_) != size or px != refs[0][0]:
            msgs.append("first frame differs (%dx%d vs %dx%d, %d octets)" % (w_, h_, size[0], size[1],
                        sum(1 for a, b in zip(px, refs[0][0]) if a != b) if (w_, h_) == size else -1))
    # the animation API: every frame
    r = subprocess.run([probe, f, out, "anim"], capture_output=True, text=True)
    if r.stdout.strip() != "OK":
        msgs.append("anim: " + r.stdout.strip())
    else:
        raw = open(out, "rb").read(); nl = raw.index(b"\n")
        w_, h_, c, lp = map(int, raw[:nl].split()); raw = raw[nl + 1:]
        dl = []
        for i in range(c):
            nl = raw.index(b"\n"); dl.append(int(raw[:nl].split()[1])); raw = raw[nl + 1:]
        if c != min(nf, 4096) or (w_, h_) != size:
            msgs.append("%d frames %dx%d, Pillow %d frames %dx%d" % (c, w_, h_, nf, size[0], size[1]))
        else:
            for i in range(c):
                mine = raw[i * w_ * h_ * 4:(i + 1) * w_ * h_ * 4]
                if mine != refs[i][0]:
                    msgs.append("frame %d differs in %d octets" % (i, sum(1 for a, b in zip(mine, refs[i][0]) if a != b)))
                if nf > 1 and dl[i] != refs[i][1]: msgs.append("delay %d: %d, Pillow %d" % (i, dl[i], refs[i][1]))
            if nf > 1 and loop is not None and lp != loop: msgs.append("loops %d, Pillow %d" % (lp, loop))
        frames_total += c; pixels += w_ * h_ * c
    if msgs:
        bad += 1; print("  DIFF %s: %s" % (os.path.basename(f), "; ".join(msgs[:3])))
print("webp: %d files (%d frames, %d pixels) decoded octet for octet like Pillow %s (libwebp), %d differ"
      % (checked, frames_total, pixels, Image.__version__, bad))
sys.exit(1 if bad else 0)
