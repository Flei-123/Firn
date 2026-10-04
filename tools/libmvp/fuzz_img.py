#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/fuzz_img.py <probe> <per_file> <file> [<file> ...] -- hostile
# input for an image probe (webp_probe, gif_probe, jpeg_probe): every cut of
# the first 300 octets, 100 random cuts and <per_file> damaged copies per
# file (bit flips, 0x00/0xFF/random octets, up to 16 hits). The probe must
# end normally with "OK" or "ERROR <kind>" in 20 s -- a crash, a panic of a
# release-safe build or a hang fails. With ANIM=1 in the environment the
# probe is asked for every frame. Deterministic (seed 1).
import os, random, subprocess, sys, tempfile
from concurrent.futures import ThreadPoolExecutor
probe, per_file, files = sys.argv[1], int(sys.argv[2]), sys.argv[3:]
tmp = tempfile.mkdtemp()
anim = ["anim"] if os.environ.get("ANIM") else []

def run(job):
    i, data, tag = job
    f = os.path.join(tmp, "f%d.bin" % i)
    open(f, "wb").write(data)
    try:
        r = subprocess.run([probe, f, f + ".out"] + anim, capture_output=True, text=True, timeout=20)
        res = r.stdout.strip()
        bad = r.returncode != 0 or not (res == "OK" or res.startswith("ERROR "))
        if bad:
            res = "CRASH rc=%d %r" % (r.returncode, (res + r.stderr)[:80])
    except subprocess.TimeoutExpired:
        res, bad = "TIMEOUT", True
    for x in (f, f + ".out"):
        try: os.unlink(x)
        except OSError: pass
    return tag, res, bad

jobs = []; rng = random.Random(1)
for f in files:
    d = open(f, "rb").read()
    cuts = list(range(0, min(len(d), 300))) + [rng.randrange(len(d)) for _ in range(100)]
    for c in cuts:
        jobs.append((len(jobs), d[:c], "%s cut %d" % (os.path.basename(f), c)))
    for _ in range(per_file):
        b = bytearray(d)
        for _ in range(rng.choice([1, 1, 2, 4, 16])):
            p = rng.randrange(len(b))
            b[p] = rng.choice([0, 255, rng.randrange(256), b[p] ^ (1 << rng.randrange(8))])
        jobs.append((len(jobs), bytes(b), "%s damaged" % os.path.basename(f)))
bad = ok = refused = 0
with ThreadPoolExecutor(8) as ex:
    for tag, res, isbad in ex.map(run, jobs):
        if isbad:
            bad += 1; print("  FAIL %s: %s" % (tag, res))
        elif res == "OK": ok += 1
        else: refused += 1
print("fuzz %s: %d inputs, %d decoded, %d refused, %d crashed" % (os.path.basename(probe), len(jobs), ok, refused, bad))
sys.exit(1 if bad else 0)
