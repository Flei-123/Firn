#!/usr/bin/env python3
import os
import subprocess
import sys
import tempfile
import zlib

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SEQ = os.path.join(ROOT, "tests", "data", "av1", "seq")
PAIR_GROUPS = int(os.environ.get("PAIR_GROUPS", "6"))
GRAIN_GROUPS = int(os.environ.get("GRAIN_GROUPS", "4"))


def tag(name):
    return name[len("seq_"):].rsplit("_", 1)[0] if name.endswith(("_hk_se.obu", "_io.obu", "_mix.obu")) else None


def stem(name):
    for suffix in ("_hk_se.obu", "_io_se.obu", "_io_er.obu", "_io.obu", "_mix.obu"):
        if name.endswith(suffix):
            return name[:-len(suffix)]
    return name


def main():
    names = sorted(os.listdir(SEQ))
    stems = []
    for n in names:
        s = stem(n)
        if s not in stems:
            stems.append(s)
    grain = [s for s in stems if s.startswith("seq_fg_")]
    pairs = [s for s in stems if not s.startswith("seq_fg_")]
    chosen = pairs[:PAIR_GROUPS] + grain[:: max(1, len(grain) // GRAIN_GROUPS)][:GRAIN_GROUPS]
    tmp = tempfile.mkdtemp()
    lines = []
    idx = 1
    for n in names:
        if stem(n) not in chosen:
            continue
        ref = os.path.join(tmp, "ref.yuv")
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", os.path.join(SEQ, n), "-f", "rawvideo", ref], check=True)
        d = open(ref, "rb").read()
        lines.append('    if !crc_of("tests/data/av1/seq/%s", 0x%08x as u32, %d) { return %d }' % (n, zlib.crc32(d), len(d), idx))
        idx += 1
    sys.stdout.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
