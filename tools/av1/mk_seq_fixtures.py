#!/usr/bin/env python3
import os
import shutil
import subprocess
import sys
import tempfile
import zlib

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
DATA = os.path.join(ROOT, "tests", "data", "av1")
OUT = os.path.join(DATA, "seq")
FIRNC = os.environ.get("FIRNC", "/root/firn/compiler/target/release/firnc")
MAX_GROUPS = int(os.environ.get("MAX_GROUPS", "16"))


def run(cmd, stdin=None):
    return subprocess.run(cmd, input=stdin, stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def leb(v):
    out = bytearray()
    while True:
        b = v & 127
        v >>= 7
        if v:
            out.append(b | 128)
        else:
            out.append(b)
            return bytes(out)


def obu(typ, payload):
    return bytes([(typ << 3) | 2]) + leb(len(payload)) + payload


def parse_obus(d):
    at = 0
    res = []
    while at < len(d):
        h = d[at]
        typ = (h >> 3) & 15
        q = at + 1 + ((h >> 2) & 1)
        sz = 0
        sh = 0
        while True:
            b = d[q]
            q += 1
            sz |= (b & 127) << sh
            sh += 7
            if not b & 128:
                break
        res.append((typ, d[q:q + sz]))
        at = q + sz
    return res


def to_bits(b):
    return [(x >> (7 - i)) & 1 for x in b for i in range(8)]


def from_bits(bits):
    bits = list(bits)
    while len(bits) % 8:
        bits.append(0)
    return bytes(sum(bits[i + j] << (7 - j) for j in range(8)) for i in range(0, len(bits), 8))


def num(v, n):
    return [(v >> (n - 1 - i)) & 1 for i in range(n)]


def val(bits):
    v = 0
    for b in bits:
        v = (v << 1) | b
    return v


def rewrite_seq(payload):
    bits = to_bits(payload)
    last = max(i for i, b in enumerate(bits) if b)
    bits = bits[:last]
    assert bits[3] == 1 and bits[4] == 1
    level = bits[5:10]
    fwb = val(bits[10:14]) + 1
    fhb = val(bits[14:18]) + 1
    sizes = bits[10:18 + fwb + fhb]
    p = 18 + fwb + fhb
    flags = bits[p:p + 3]
    rest = bits[p + 3:]
    new = bits[0:3] + [0, 0, 0, 0] + num(0, 5) + num(0, 12) + level
    if val(level) > 7:
        new += [0]
    new += sizes + [0] + flags + [0, 0, 0, 0, 1, 0, 0, 1, 1] + num(6, 3) + rest + [1]
    return from_bits(new)


class Frame:
    def __init__(self, payload, refresh_pos, tile_pos, hdr_bits):
        self.bits = to_bits(payload)
        self.hdr = self.bits[:hdr_bits]
        self.tile = payload[(hdr_bits + 7) // 8:]
        self.rp = refresh_pos
        self.tp = tile_pos
        self.dis = self.hdr[0]

    def build(self, kind, order, refresh):
        if kind == "key_shown":
            pre = [0, 0, 0, 1]
        elif kind == "key_hidden":
            pre = [0, 0, 0, 0, 1, 0]
        elif kind == "intra_shown":
            pre = [0, 1, 0, 1, 0]
        elif kind == "intra_shown_er":
            pre = [0, 1, 0, 1, 1]
        elif kind == "intra_hidden":
            pre = [0, 1, 0, 0, 1, 0]
        else:
            raise ValueError(kind)
        h = pre + self.hdr[:self.rp] + [0] + num(order, 7)
        if kind != "key_shown":
            h += num(refresh, 8)
        if kind == "intra_shown_er":
            for _ in range(8):
                h += num(0, 7)
        h += self.hdr[self.rp:self.tp]
        if self.dis == 0:
            h += [1]
        h += self.hdr[self.tp:]
        return obu(6, from_bits(h) + self.tile)


def show_existing(idx):
    return obu(3, bytes([0x80 | (idx << 4) | 0x08]))


TD = bytes([0x12, 0x00])


def stream(seq, units):
    out = TD + obu(1, rewrite_seq(seq))
    first = True
    for tu in units:
        if not first:
            out += TD
        first = False
        out += b"".join(tu)
    return out


def probe(tool, path):
    r = run([tool], open(path, "rb").read())
    info = {}
    for line in r.stdout.decode().splitlines():
        if line.startswith("frame "):
            for kv in line.split()[1:]:
                k, v = kv.split("=")
                info[k] = int(v)
    return info


def main():
    os.environ["FIRNLIB"] = os.path.join(ROOT, "lib")
    tmp = tempfile.mkdtemp()
    seqhdr = os.path.join(tmp, "seqhdr")
    r = run([FIRNC, os.path.join(ROOT, "tools/av1/seqhdr.fi"), "-o", seqhdr])
    if r.returncode:
        sys.stderr.write(r.stderr.decode())
        sys.exit(1)
    groups = {}
    for f in sorted(os.listdir(DATA)):
        if not f.endswith(".avif"):
            continue
        o = os.path.join(tmp, "x.obu")
        if run(["ffmpeg", "-v", "error", "-y", "-i", os.path.join(DATA, f), "-c", "copy", "-f", "obu", o]).returncode:
            continue
        d = open(o, "rb").read()
        obus = parse_obus(d)
        types = [t for t, _ in obus]
        if types != [2, 1, 6]:
            continue
        info = probe(seqhdr, o)
        if not info or info.get("ok") != 1 or info.get("ftype") != 0 or len(d) > 40000:
            continue
        seq = obus[1][1]
        if (seq[0] >> 3) & 3 != 3:
            continue
        fr = Frame(obus[2][1], info["refresh_pos"], info["tile_pos"], info["hdr_bits"])
        groups.setdefault(seq, []).append((f, fr))
    pairs = [g for g in groups.items() if len(g[1]) >= 2]
    pairs.sort(key=lambda g: g[1][0][0])
    grain = [(k, v * 2) for k, v in groups.items() if len(v) == 1 and v[0][0].startswith("fg_")]
    grain.sort(key=lambda g: g[1][0][0])
    step = max(1, len(grain) // 8)
    picked = pairs[:MAX_GROUPS] + grain[::step][:8]
    os.makedirs(OUT, exist_ok=True)
    for f in os.listdir(OUT):
        os.remove(os.path.join(OUT, f))
    names = []
    for seq, members in picked:
        (na, a), (nb, b) = members[0], members[1]
        tag = na.replace(".avif", "")
        variants = {
            "hk_se": [[a.build("key_hidden", 0, 1), show_existing(0)]],
            "io": [[a.build("key_shown", 0, 255)], [b.build("intra_shown", 1, 2)]],
            "io_er": [[a.build("key_shown", 0, 255)], [b.build("intra_shown_er", 1, 2)]],
            "io_se": [[a.build("key_hidden", 0, 1), b.build("intra_hidden", 1, 2), show_existing(1)], [show_existing(1)],
                      [show_existing(0)], [show_existing(1)]],
            "mix": [[a.build("key_shown", 0, 255)], [b.build("intra_hidden", 2, 4), show_existing(2)],
                    [b.build("intra_shown", 3, 8)], [show_existing(2)]],
        }
        for v, units in variants.items():
            name = "seq_%s_%s.obu" % (tag, v)
            open(os.path.join(OUT, name), "wb").write(stream(seq, units))
            names.append(name)
    seq0, members0 = picked[0]
    a0 = members0[0][1]
    bad = {
        "seq_bad_empty_se.obu": [[show_existing(0)]],
        "seq_bad_nonshowable.obu": [[a0.build("key_shown", 0, 255)], [show_existing(0)]],
        "seq_bad_empty_slot.obu": [[a0.build("key_hidden", 0, 1)], [show_existing(3)]],
    }
    for name, units in bad.items():
        open(os.path.join(OUT, name), "wb").write(stream(seq0, units))
        names.append(name)
    print("\n".join(names))
    shutil.rmtree(tmp)


if __name__ == "__main__":
    main()
