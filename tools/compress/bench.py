#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/compress/bench.py -- throughput of lib/compress next to the reference
libraries (python-lz4, python-zstandard, lzma, bz2, brotli, zlib = liblz4,
libzstd, liblzma, libbz2, libbrotli, zlib), on the same machine, in the same
minute, best of 3.

    bench.py <bench-binary> <corpus> [<corpus> ...]      (prints a markdown table)

The bench binary is tools/compress/bench.fi built with --opt-level=release-fast.
MB/s = 1,000,000 octets of the UNCOMPRESSED corpus per second. Numbers on a busy
machine are lower and noisy; the table in docs/COMPRESSION.md says when it was
made. Needs lz4, zstandard, brotli (a venv is fine).
"""
import os, sys, subprocess, time, tempfile, shutil, zlib, bz2, lzma, gzip

BENCH = os.path.abspath(sys.argv[1])
CORPORA = sys.argv[2:]
import lz4.frame as lf
import zstandard
import brotli

W = tempfile.mkdtemp(prefix="compress-bench-")


def best(fn, n=3):
    b = 1e9
    r = None
    for _ in range(n):
        t = time.perf_counter()
        r = fn()
        b = min(b, time.perf_counter() - t)
    return b, r


def mbs(n, secs):
    return n / secs / 1e6


try:
    for corpus in CORPORA:
        data = open(corpus, "rb").read()
        n = len(data)
        # reference: compress and decompress
        ref = {}

        def addref(name, comp, decomp):
            tc, z = best(lambda: comp(data))
            td, _ = best(lambda: decomp(z))
            ref[name] = (mbs(n, tc), mbs(n, td), len(z), z)

        addref("lz4", lambda d: lf.compress(d, compression_level=0), lf.decompress)
        for lv in (1, 3, 9, 19):
            c = zstandard.ZstdCompressor(level=lv)
            dd = zstandard.ZstdDecompressor()
            addref("zstd-%d" % lv, c.compress, lambda z, dd=dd: dd.decompress(z, max_output_size=n + 1))
        addref("xz-1", lambda d: lzma.compress(d, preset=1), lzma.decompress)
        addref("xz-6", lambda d: lzma.compress(d, preset=6), lzma.decompress)
        addref("gzip-6", lambda d: gzip.compress(d, 6), gzip.decompress)
        addref("bz2-9", lambda d: bz2.compress(d, 9), bz2.decompress)
        addref("br-6", lambda d: brotli.compress(d, quality=6), brotli.decompress)
        # files for the decoders that have no encoder here
        args = []
        for tag, key in (("bz2", "bz2-9"), ("br", "br-6")):
            p = os.path.join(W, "c." + tag)
            open(p, "wb").write(ref[key][3])
            args.append("%s=%s" % (tag, p))
        out = subprocess.run([BENCH, corpus] + args, capture_output=True, text=True, timeout=3600).stdout
        ours = {}
        for line in out.splitlines():
            f = line.split()
            if f[0] == "ENC":
                ours.setdefault(f[1], {})["enc"] = (mbs(n, int(f[4]) / 1e6), int(f[3]))
            elif f[0] == "DEC":
                ours.setdefault(f[1], {})["dec"] = (mbs(n, int(f[3]) / 1e6), f[4] == "1")
        print("\n### %s (%d octets)\n" % (os.path.basename(corpus), n))
        print("| codec | compress: lib | reference | ratio lib | reference | decompress: lib | reference | right |")
        print("|---|---:|---:|---:|---:|---:|---:|:--:|")
        rows = [("lz4", "lz4", "lz4"), ("zstd-1", "zstd-1", "zstd-1"), ("zstd-3", "zstd-3", "zstd-3"), ("zstd-9", "zstd-9", "zstd-9"),
                ("zstd-19", "zstd-19", "zstd-19"), ("xz-1", "xz-1", "xz-1"), ("xz-6", "xz-6", "xz-6"), ("gzip-6", "gzip-6", "gzip-6")]
        for name, oname, rname in rows:
            o = ours.get(oname + "/own") or {}
            e = ours.get(oname, {}).get("enc")
            d = ours.get(oname + "/own", {}).get("dec")
            r = ref[rname]
            print("| %s | %.1f | %.1f | %.3f | %.3f | %.1f | %.1f | %s |" % (
                name, e[0], r[0], e[1] / n, r[2] / n, d[0], r[1], "yes" if d[1] else "NO"))
        for name, oname, rname in (("bzip2 -9", "bz2", "bz2-9"), ("brotli 6", "br", "br-6")):
            d = ours.get(oname, {}).get("dec")
            r = ref[rname]
            print("| %s | -- | %.1f | -- | %.3f | %.1f | %.1f | %s |" % (name, r[0], r[2] / n, d[0], r[1], "yes" if d[1] else "NO"))
finally:
    shutil.rmtree(W, ignore_errors=True)
