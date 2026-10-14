#!/usr/bin/env python3
# Decode the JPEGs written by tools/photo/jpegenc_dump.fi with Pillow (libjpeg) and print PSNR vs the source.
# Usage: python3 -I tools/photo/jpegenc_pil.py   (compiles the dump tool with $FIRNC, uses a temp dir)
import os, subprocess, sys, tempfile, math
from PIL import Image
root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
firnc = os.environ.get("FIRNC", os.path.join(root, "compiler/target/release/firnc"))
with tempfile.TemporaryDirectory() as d:
    exe = os.path.join(d, "dump")
    env = dict(os.environ, FIRNLIB=os.path.join(root, "lib"))
    subprocess.check_call([firnc, "--opt-level=dev-fast", "-o", exe, os.path.join(root, "tools/photo/jpegenc_dump.fi")], env=env)
    subprocess.check_call([exe, d + "/"])
    ref = Image.open(os.path.join(d, "ref.ppm")).convert("RGB")
    rb = ref.tobytes()
    bad = 0
    for name in ("q90_420", "q95_420", "q90_444", "q95_444"):
        im = Image.open(os.path.join(d, name + ".jpg")); im.load()
        assert im.size == ref.size and im.format == "JPEG", name
        db = im.convert("RGB").tobytes()
        mse = sum((a - b) ** 2 for a, b in zip(rb, db)) / len(rb)
        p = 99.0 if mse == 0 else 10 * math.log10(255 * 255 / mse)
        # reference: Pillow's own encoder at the same quality
        import io
        bio = io.BytesIO(); ref.save(bio, "JPEG", quality=int(name[1:3]), subsampling=2 if name.endswith("420") else 0)
        pb = Image.open(io.BytesIO(bio.getvalue())).convert("RGB").tobytes()
        mse2 = sum((a - b) ** 2 for a, b in zip(rb, pb)) / len(rb)
        p2 = 10 * math.log10(255 * 255 / mse2)
        size = os.path.getsize(os.path.join(d, name + ".jpg"))
        print(f"{name}: firn {p:.2f} dB {size} B | pillow {p2:.2f} dB {len(bio.getvalue())} B")
        if p < p2 - 1.0: bad += 1
    print("ok" if bad == 0 else "FAIL")
    sys.exit(bad)
