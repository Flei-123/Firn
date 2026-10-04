#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/compress/memtest.py -- does the streaming reader really stream?

    memtest.py <memtest-binary> [megabytes]       (default 300)

A random 1 MiB block repeated until `megabytes` MiB are reached, compressed by the
reference tools (zstd, xz, bz2, gzip, lz4, brotli), then decoded by
tools/compress/memtest.fi through the streaming API with the output thrown away.
The octet count and the xxHash64 of the output must match, and the peak resident
set must stay far below the output size (window + one unit).
"""
import os, sys, subprocess, tempfile, shutil, random, hashlib
import xxhash, bz2, lzma, gzip, zlib
import lz4.frame as lf, brotli

BIN = os.path.abspath(sys.argv[1])
MB = int(sys.argv[2]) if len(sys.argv) > 2 else 300
W = tempfile.mkdtemp(prefix="compress-mem-")
rnd = random.Random(5)
block = bytes(rnd.getrandbits(8) for _ in range(1 << 20))
total = MB << 20
h = xxhash.xxh64()
for _ in range(MB):
    h.update(block)
want = h.hexdigest().lstrip("0") or "0"
fails = []


def run(name, fmt, path, limit_kb):
    r = subprocess.run([BIN, fmt, path], capture_output=True, text=True, timeout=1200)
    f = r.stdout.split()
    ok = len(f) >= 6 and int(f[1]) == total and f[3] == want
    kb = int(f[5]) if len(f) >= 6 else -1
    good = ok and 0 < kb < limit_kb
    print("  %-8s %s  output %d MiB, peak %d MiB (limit %d MiB)" % (name, "ok  " if good else "FAIL", total >> 20, kb >> 10, limit_kb >> 10))
    if not good:
        fails.append(name)


try:
    def stream(cmd, name):
        p = os.path.join(W, name)
        with open(p, "wb") as f:
            pr = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=f)
            for _ in range(MB):
                pr.stdin.write(block)
            pr.stdin.close()
            pr.wait()
        return p
    # zstd: window 8 MiB at level 3 (--long=27 asks for 128 MiB: the decoder's cap is the default 128 MiB)
    run("zstd", "auto", stream(["zstd", "-q", "-c", "-3"], "m.zst"), 80 << 10)
    run("xz", "auto", stream(["xz", "-c", "-3"], "m.xz"), 80 << 10)      # preset 3: 4 MiB dictionary
    run("gzip", "gz", stream(["gzip", "-c", "-6"], "m.gz"), 20 << 10)
    run("bzip2", "auto", stream(["bzip2", "-c", "-9"], "m.bz2"), 50 << 10)
    p = os.path.join(W, "m.lz4")
    with open(p, "wb") as f:
        c = lf.LZ4FrameCompressor(block_size=lf.BLOCKSIZE_MAX4MB, content_checksum=True)
        f.write(c.begin())
        for _ in range(MB):
            f.write(c.compress(block))
        f.write(c.flush())
    run("lz4", "auto", p, 40 << 10)
    p = os.path.join(W, "m.br")
    with open(p, "wb") as f:
        c = brotli.Compressor(quality=1, lgwin=22)
        for _ in range(MB):
            f.write(c.process(block))
        f.write(c.finish())
    run("brotli", "br", p, 80 << 10)
finally:
    shutil.rmtree(W, ignore_errors=True)
print("memtest: %d formats failed" % len(fails))
sys.exit(1 if fails else 0)
