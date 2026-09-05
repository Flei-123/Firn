#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/stdlib81/deflate_check.py -- lib/std/deflate.fi against zlib/gzip.

The whole point is that NOTHING here is judged by this repository:

  * everything Firn PACKS is unpacked by `python3 zlib`/`gzip` (and by the
    `gunzip` binary for the gzip frame),
  * everything zlib/gzip PACK is unpacked by Firn,
  * and the round trip through Firn alone is checked too -- because a
    compressor and a decompressor that are wrong in the SAME way would pass
    the round trip and fail the first two.

The corpus deliberately contains the cases that break implementations:
the EMPTY input (a stream with no data still needs a block), ONE octet,
data that CANNOT be compressed (the output must not run away -- stored
blocks), data that consists of ONE repeated octet (maximum match length,
the copy that reads what it just wrote), and real files.

Usage: deflate_check.py <deflate_cli> <work> <file>...
"""
import gzip
import os
import subprocess
import sys
import zlib


def run(cli, mode, level, src, dst):
    return subprocess.run([cli, mode, str(level), src, dst],
                          capture_output=True).returncode


def main():
    if len(sys.argv) < 4:
        print("usage: deflate_check.py <deflate_cli> <work> <file>...")
        return 2
    cli, work = sys.argv[1], sys.argv[2]
    files = sys.argv[3:]
    os.makedirs(work, exist_ok=True)
    errors = []
    rows = []

    for path in files:
        data = open(path, "rb").read()
        name = os.path.basename(path)
        best = None
        for level in (0, 1, 6, 9):
            packed = os.path.join(work, "p.bin")
            back = os.path.join(work, "b.bin")

            # 1. Firn packs -- zlib unpacks.
            if run(cli, "zlibc", level, path, packed) != 0:
                errors.append("%s L%d: zlib_compress failed" % (name, level))
                continue
            blob = open(packed, "rb").read()
            try:
                if zlib.decompress(blob) != data:
                    errors.append("%s L%d: zlib decompressed something else"
                                  % (name, level))
            except Exception as e:                    # noqa: BLE001
                errors.append("%s L%d: zlib refused the stream (%s)"
                              % (name, level, e))
            if level == 6:
                best = len(blob)

            # 2. Firn packs gzip -- the gzip BINARY unpacks.
            if run(cli, "gzipc", level, path, packed) != 0:
                errors.append("%s L%d: gzip_compress failed" % (name, level))
            else:
                p = subprocess.run(["gunzip", "-c", packed],
                                   capture_output=True)
                if p.returncode != 0 or p.stdout != data:
                    errors.append("%s L%d: gunzip did not get it back"
                                  % (name, level))

            # 3. Firn packs raw -- zlib unpacks raw (window -15).
            if run(cli, "rawc", level, path, packed) != 0:
                errors.append("%s L%d: deflate failed" % (name, level))
            else:
                try:
                    if zlib.decompress(open(packed, "rb").read(), -15) != data:
                        errors.append("%s L%d: raw stream is wrong"
                                      % (name, level))
                except Exception as e:                # noqa: BLE001
                    errors.append("%s L%d: raw refused (%s)" % (name, level, e))

            # 4. zlib/gzip pack -- FIRN unpacks.
            zf = os.path.join(work, "z.bin")
            open(zf, "wb").write(zlib.compress(data, level))
            if run(cli, "zlibd", 0, zf, back) != 0 or open(back, "rb").read() != data:
                errors.append("%s L%d: Firn could not read zlib's stream"
                              % (name, level))
            open(zf, "wb").write(gzip.compress(data, level))
            if run(cli, "gzipd", 0, zf, back) != 0 or open(back, "rb").read() != data:
                errors.append("%s L%d: Firn could not read gzip's stream"
                              % (name, level))
            c = zlib.compressobj(level, zlib.DEFLATED, -15)
            open(zf, "wb").write(c.compress(data) + c.flush())
            if run(cli, "rawd", 0, zf, back) != 0 or open(back, "rb").read() != data:
                errors.append("%s L%d: Firn could not read the raw stream"
                              % (name, level))

            # 5. Firn packs, FIRN unpacks.
            if run(cli, "zlibc", level, path, packed) == 0:
                if run(cli, "zlibd", 0, packed, back) != 0 or \
                        open(back, "rb").read() != data:
                    errors.append("%s L%d: Firn's own round trip failed"
                                  % (name, level))

        if best is not None:
            ref = len(zlib.compress(data, 6))
            rows.append((name, len(data), best, ref))

    for name, raw, mine, ref in rows:
        pct = 100.0 * mine / raw if raw else 0.0
        rel = 100.0 * mine / ref if ref else 0.0
        print("  %-22s %9d -> %8d octets  %5.1f%%  (zlib -6: %8d, %.1f%%)"
              % (name, raw, mine, pct, ref, rel))

    # THE COUNTER-CHECKS. Without them the rest proves nothing: a
    # decompressor that accepts garbage would pass everything above.
    bad = [("a truncated stream", zlib.compress(b"hello world" * 50)[:12]),
           ("a wrong Adler-32",
            zlib.compress(b"hello world")[:-1] + b"\xff"),
           ("a header that is not zlib", b"\x00\x00\x00\x00\x00\x00"),
           ("empty", b"")]
    refused = 0
    for label, blob in bad:
        f = os.path.join(work, "bad.bin")
        open(f, "wb").write(blob)
        if run(cli, "zlibd", 0, f, os.path.join(work, "bad.out")) != 0:
            refused += 1
        else:
            errors.append("counter-check: %s was ACCEPTED" % label)
    print("  counter-checks refused %d / %d" % (refused, len(bad)))

    for e in errors[:10]:
        print("  FAIL %s" % e)
    print("  RESULT %s (%d files, %d errors)"
          % ("ok" if not errors else "FAILED", len(files), len(errors)))
    return 0 if not errors else 1


if __name__ == "__main__":
    sys.exit(main())
