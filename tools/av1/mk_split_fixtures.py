#!/usr/bin/env python3
import importlib.util
import os
import subprocess
import sys
import tempfile
import zlib

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SEQ = os.path.join(ROOT, "tests", "data", "av1", "seq")
OUT = os.path.join(ROOT, "tests", "data", "av1", "split")
FIRNC = os.environ.get("FIRNC", "/root/firn/compiler/target/release/firnc")
COUNT = int(os.environ.get("SPLIT_COUNT", "6"))

spec = importlib.util.spec_from_file_location("mk", os.path.join(ROOT, "tools", "av1", "mk_seq_fixtures.py"))
mk = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mk)


def header_bits(dump, data):
    out = subprocess.run([dump], input=data, stdout=subprocess.PIPE).stdout.decode().split("\n")
    return [int(dict(kv.split("=") for kv in ln.split())["hdrbits"]) for ln in out if ln.strip()]


def split(data, bits_list):
    out = bytearray()
    k = 0
    for typ, payload in mk.parse_obus(data):
        if typ != 6:
            out += mk.obu(typ, payload)
            continue
        nb = bits_list[k]
        k += 1
        hb = mk.to_bits(payload)[:nb] + [1]
        while len(hb) % 8:
            hb.append(0)
        head = bytes(int("".join(map(str, hb[i:i + 8])), 2) for i in range(0, len(hb), 8))
        out += mk.obu(3, head)
        out += mk.obu(4, payload[(nb + 7) // 8:])
    return bytes(out)


def main():
    os.makedirs(OUT, exist_ok=True)
    tmp = tempfile.mkdtemp()
    dump = os.path.join(tmp, "hd")
    subprocess.run([FIRNC, os.path.join(ROOT, "tools/av1/hdrdump.fi"), "-o", dump], env=dict(os.environ, FIRNLIB=os.path.join(ROOT, "lib")), check=True)
    names = sorted(n for n in os.listdir(SEQ) if n.endswith(".obu"))
    step = max(1, len(names) // COUNT)
    lines = []
    for n in names[::step][:COUNT]:
        data = open(os.path.join(SEQ, n), "rb").read()
        bl = header_bits(dump, data)
        out = split(data, bl)
        name = "split_" + n
        open(os.path.join(OUT, name), "wb").write(out)
        ref = os.path.join(tmp, "ref.yuv")
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", os.path.join(OUT, name), "-f", "rawvideo", ref], check=True)
        d = open(ref, "rb").read()
        lines.append((name, zlib.crc32(d), len(d)))
    for i, (name, crc, size) in enumerate(lines):
        sys.stdout.write('    if !crc_of("tests/data/av1/split/%s", 0x%08x as u32, %d) { return %d }\n' % (name, crc, size, i + 1))


if __name__ == "__main__":
    main()
