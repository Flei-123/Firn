#!/usr/bin/env python3
"""Cross-check image.picture against Pillow: decode (png/jpeg/webp/gif) and BILINEAR resize.
usage: check.py <firnc> <firnlib> <probe.fi> [cases]   (works in its own temp dir, removed at the end)"""
import os, random, shutil, subprocess, sys, tempfile
from PIL import Image

firnc, firnlib, probe = sys.argv[1], sys.argv[2], os.path.abspath(sys.argv[3])
cases = int(sys.argv[4]) if len(sys.argv) > 4 else 60
d = tempfile.mkdtemp(prefix="piccross-")
try:
    rnd = random.Random(7)
    man = []
    for k in range(cases):
        w, h = rnd.randint(1, 90), rnd.randint(1, 70)
        mode = rnd.choice(["RGB", "RGBA", "L", "P"])
        base = Image.new("RGB", (w, h))
        px = base.load()
        for y in range(h):
            for x in range(w):
                px[x, y] = ((x * 5 + k * 3) % 256, (y * 7 + x) % 256, rnd.randint(0, 255) if rnd.random() < .2 else (x + y) % 256)
        im = base.convert("RGBA") if mode == "RGBA" else base.convert(mode)
        if mode == "RGBA":
            a = Image.effect_noise((w, h), 90) if w > 1 else Image.new("L", (w, h), 200)
            im.putalpha(a)
        fmt = rnd.choice(["png", "jpg", "webp", "gif"])
        if fmt == "jpg":
            im = im.convert("RGB")
        if fmt == "gif":
            im = im.convert("P")
        name = f"c{k}.{fmt}"
        if fmt == "webp":
            im.save(os.path.join(d, name), lossless=True)
        else:
            im.save(os.path.join(d, name))
        man.append((name, max(1, w // rnd.randint(1, 3) + rnd.randint(0, 5)), max(1, h // rnd.randint(1, 3) + rnd.randint(0, 5))))
    with open(os.path.join(d, "manifest.txt"), "w") as f:
        f.write("".join(f"{n} {w} {h}\n" for n, w, h in man))
    env = dict(os.environ, FIRNLIB=firnlib)
    exe = os.path.join(d, "probe")
    r = subprocess.run([firnc, "--opt-level=dev-fast", "-o", exe, probe], capture_output=True, text=True, env=env)
    if r.returncode:
        print(r.stderr[:500]); sys.exit(2)
    subprocess.run([exe], cwd=d, check=True, capture_output=True)

    def read(path):
        b = open(path, "rb").read()
        nl = b.index(b"\n")
        w, h = map(int, b[:nl].split())
        return w, h, b[nl + 1:]

    def ref_alpha(n):
        r = Image.open(os.path.join(d, n))
        return r.mode in ("RGBA", "LA") or "transparency" in r.info

    bad = checked = 0
    worst = {"dec": 0, "rs": 0}
    for name, rw, rh in man:
        ref = Image.open(os.path.join(d, name))
        ref = ref.convert("RGBA")
        for tag, want in (("dec", ref), ("rs", ref.resize((rw, rh), Image.BILINEAR))):
            w, h, data = read(os.path.join(d, name + "." + tag))
            checked += 1
            if (w, h) != want.size or len(data) != w * h * 4:
                bad += 1; print("SIZE", name, tag, (w, h), want.size); continue
            tol = 3 if name.endswith("jpg") else 1
            if tag == "rs" and ref_alpha(name): tol = 3
            wb = want.tobytes()
            # compare on white so that the hidden colour of transparent pixels does not count
            diff = 0
            for i in range(0, len(wb), 4):
                a1, a2 = wb[i + 3], data[i + 3]
                diff = max(diff, abs(a1 - a2))
                for c in range(3):
                    v1 = (wb[i + c] * a1 + 255 * (255 - a1)) // 255
                    v2 = (data[i + c] * a2 + 255 * (255 - a2)) // 255
                    diff = max(diff, abs(v1 - v2))
            worst[tag] = max(worst[tag], diff)
            if diff > (tol if tag == "dec" else tol + 1 - (1 if tol == 3 else 0)):
                bad += 1; print("DIFF", name, tag, diff)
    print(f"checked {checked} cases, {bad} deviations, worst decode {worst['dec']}, worst resize {worst['rs']}")
    sys.exit(1 if bad else 0)
finally:
    shutil.rmtree(d, ignore_errors=True)
