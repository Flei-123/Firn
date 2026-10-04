#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/compress/fuzz.py -- mutate valid streams and feed them to the probe.

    fuzz.py <probe> <iterations> [seed]

Takes the fixtures of tests/data/compress (every format, every flavour), applies
random mutations (bit flips, byte and word overwrites, deletions, insertions,
duplications of a stretch, splices of two streams, truncation) and runs the
probe on the result, whole buffer (`d`, with a 16 MiB limit) and streaming
(`ds`, 8191 octets at a time). The answer must be `OK <n>` or `ERROR <kind>`:
a crash (a trap in a release-safe build is an arithmetic overflow or an index
out of range that the format code did not foresee), a hang, a missing answer or
a limit that was passed are reported with the input saved under /tmp.
Build the probe with --opt-level=release-safe: that is where overflows trap.
"""
import os, sys, random, subprocess, tempfile, shutil, glob

PROBE = os.path.abspath(sys.argv[1])
N = int(sys.argv[2])
SEED = int(sys.argv[3]) if len(sys.argv) > 3 else 1
root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
fx = os.path.join(root, "tests", "data", "compress")
W = tempfile.mkdtemp(prefix="compress-fuzz-")
rnd = random.Random(SEED)

FORMATS = {  # file glob -> probe format
    "plain*.lz4": "lz4", "plain*.zst": "zstd", "record*.zst": "zstd", "plain*.xz": "xz", "mix_*.xz": "xz",
    "plain.lzma": "lzma", "plain*.bz2": "bz2", "random_block.bz2": "bz2", "runs.bz2": "bz2",
    "plain*.br": "br", "words.br": "br", "stored.br": "br", "plain.gz": "gz", "plain_multi.gz": "gz",
    "plain.zlib": "zlib", "plain.deflate": "deflate", "pack.tar.zst": "auto", "pack.tar.xz": "auto",
    "pack.tar.bz2": "auto", "pack.tar.lz4": "auto", "pack.tar.gz": "auto",
}
corpus = []
for pat, fmt in FORMATS.items():
    for f in sorted(glob.glob(os.path.join(fx, pat))):
        corpus.append((fmt, os.path.basename(f), open(f, "rb").read()))
sys.stderr.write("fuzz: %d seed streams, %d iterations, seed %d\n" % (len(corpus), N, SEED))


def mutate(data, other):
    b = bytearray(data)
    for _ in range(rnd.choice((1, 1, 1, 2, 3, 6))):
        if not b:
            break
        k = rnd.randrange(12)
        i = rnd.randrange(len(b))
        if k <= 2:
            b[i] ^= 1 << rnd.randrange(8)
        elif k == 3:
            b[i] = rnd.getrandbits(8)
        elif k == 4:
            b[i] = rnd.choice((0, 0xFF, 0x7F, 0x80, 1))
        elif k == 5:
            j = min(len(b), i + rnd.choice((1, 2, 3, 4, 8)))
            del b[i:j]
        elif k == 6:
            b[i:i] = bytes(rnd.getrandbits(8) for _ in range(rnd.choice((1, 2, 4, 9))))
        elif k == 7:
            j = min(len(b), i + rnd.randrange(1, 40))
            b[i:i] = b[i:j]  # duplicate a stretch
        elif k == 8 and len(b) > 4:
            w = rnd.choice((0, 0xFFFFFFFF, 0x7FFFFFFF, 0x80000000, 0x10000, 0xFFFF, rnd.getrandbits(32)))
            b[i:i + 4] = w.to_bytes(4, "little")[: len(b[i:i + 4])]
        elif k == 9:
            j = rnd.randrange(len(other))
            b[i:] = other[j:j + rnd.randrange(1, 300)] + b[i + rnd.randrange(0, 20):]
        elif k == 10:
            del b[i:]
        else:
            b[i] = (b[i] + rnd.choice((-1, 1, 16, -16))) & 255
    return bytes(b)


bad = []
count = 0
try:
    for it in range(N):
        fmt, name, data = rnd.choice(corpus)
        other = rnd.choice(corpus)[2]
        z = mutate(data, other)
        open(os.path.join(W, "in.bin"), "wb").write(z)
        for op in ("d", "ds"):
            args = [PROBE, op, fmt, "in.bin", "out.bin", "16777216" if op == "d" else "8191"]
            if os.path.exists(os.path.join(W, "out.bin")):
                os.remove(os.path.join(W, "out.bin"))
            try:
                r = subprocess.run(args, capture_output=True, timeout=30, cwd=W)
                out = r.stdout.decode(errors="replace").strip().splitlines()
                ok = r.returncode == 0 and out and (out[-1].startswith("OK") or out[-1].startswith("ERROR"))
                if ok and out[-1].startswith("OK") and op == "d" and os.path.getsize(os.path.join(W, "out.bin")) > 16777216:
                    ok = False
                if not ok:
                    keep = "/tmp/compress-fuzz-crash-%s-%d-%s.bin" % (fmt, it, op)
                    shutil.copyfile(os.path.join(W, "in.bin"), keep)
                    bad.append("%s %s %s rc=%d %s -> %s" % (fmt, name, op, r.returncode, r.stderr[-160:], keep))
            except subprocess.TimeoutExpired:
                keep = "/tmp/compress-fuzz-hang-%s-%d-%s.bin" % (fmt, it, op)
                shutil.copyfile(os.path.join(W, "in.bin"), keep)
                bad.append("%s %s %s HANG -> %s" % (fmt, name, op, keep))
            count += 1
        if it % 500 == 499:
            sys.stderr.write("  %d iterations, %d problems\n" % (it + 1, len(bad)))
finally:
    shutil.rmtree(W, ignore_errors=True)
print("fuzz: %d probe runs, %d problems" % (count, len(bad)))
for b in bad[:30]:
    print("  PROBLEM", b)
sys.exit(1 if bad else 0)
