import math
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from jxlbits import to_bits, to_bytes
from PIL import Image

FIELDS = 12
USE_LF = 32
TARGET = (520, 520)


def make_image(path, w, h):
    im = Image.new("RGB", (w, h))
    px = []
    for y in range(h):
        for x in range(w):
            px.append((int(127 + 100 * math.sin(x / 23.0 + y / 41.0)), int(127 + 90 * math.cos(y / 17.0 + x / 71.0)), (x * 3 + y * 2) & 255))
    im.putdata(px)
    im.save(path)


def encode(cjxl, work, name, w, h, progressive_dc):
    png = "%s/%s.png" % (work, name)
    jxl = "%s/%s.jxl" % (work, name)
    make_image(png, w, h)
    subprocess.run([cjxl, png, jxl, "-d", "1", "-e", "7", "--progressive_dc=%d" % progressive_dc], check=True, capture_output=True)
    return jxl


def probe_frames(probe, work, path):
    data = open(path, "rb").read()
    assert data[:2] == b"\xff\x0a"
    open(work + "/p.jxl", "wb").write(data)
    out = subprocess.run([probe], cwd=work, check=True, capture_output=True, text=True).stdout.split()
    nums = [int(v) for v in out]
    frames = []
    for i in range(0, len(nums), FIELDS):
        f = nums[i:i + FIELDS]
        frames.append({"start": f[0], "body": f[2], "total": f[3], "type": f[4], "enc": f[5], "flags": f[6], "dc": f[7], "w": f[8], "h": f[9]})
    return data, frames


def frame_bytes(data, f):
    return bytearray(data[f["start"]:f["start"] + f["body"] + f["total"]])


class Cursor:
    def __init__(self, bits):
        self.bits = bits
        self.pos = 0

    def take(self, n):
        v = 0
        for i in range(n):
            v |= self.bits[self.pos + i] << i
        self.pos += n
        return v


def dc_level_bit_position(raw, f):
    cur = Cursor(to_bits(bytes(raw[:f["body"]])))
    assert cur.take(1) == 0
    assert cur.take(2) == 1
    enc = cur.take(1)
    sel = cur.take(2)
    if sel == 1:
        cur.take(4)
    if sel == 2:
        cur.take(8)
    assert sel in (0, 1, 2)
    if not f["flags"] & USE_LF:
        cur.take(2)
    if enc == 1:
        cur.take(2)
    elif True:
        cur.take(6)
    assert cur.take(2) == 0
    return cur.pos


def relabel(raw, f, level):
    pos = dc_level_bit_position(raw, f)
    bits = to_bits(bytes(raw))
    code = level - 1
    assert bits[pos] | (bits[pos + 1] << 1) == f["dc"] - 1
    bits[pos] = code & 1
    bits[pos + 1] = (code >> 1) & 1
    return bytearray(to_bytes(bits))


def build(cjxl, probe, work, tag, levels):
    tw, th = TARGET
    t_path = encode(cjxl, work, "target", tw, th, 2)
    tdata, tframes = probe_frames(probe, work, t_path)
    assert [f["dc"] for f in tframes] == [2, 1, 0]
    head = tdata[:tframes[0]["start"]]
    frames = []
    for level in sorted(levels, reverse=True):
        w = -(-tw // (8 ** (level - 1)))
        h = -(-th // (8 ** (level - 1)))
        if level == max(levels):
            d_path = encode(cjxl, work, "donor_m%d" % level, w, h, 1)
            ddata, dframes = probe_frames(probe, work, d_path)
            donor = dframes[0]
        else:
            d_path = encode(cjxl, work, "donor_v%d" % level, w, h, 2)
            ddata, dframes = probe_frames(probe, work, d_path)
            donor = dframes[1]
        assert donor["dc"] == 1
        want_w = -(-tw // (8 ** level))
        want_h = -(-th // (8 ** level))
        assert (donor["w"], donor["h"]) == (want_w, want_h), (level, donor, want_w, want_h)
        frames.append(relabel(frame_bytes(ddata, donor), donor, level))
    rest = [frame_bytes(tdata, f) for f in tframes[1:]]
    out = bytearray(head)
    for fr in frames + rest:
        out += fr
    return bytes(out)


def main():
    cjxl, probe, work, out_dir = sys.argv[1:5]
    for tag, levels in (("dc3", [3, 2]), ("dc4", [4, 3, 2])):
        data = build(cjxl, probe, work, tag, levels)
        open("%s/%s.jxl" % (out_dir, tag), "wb").write(data)


if __name__ == "__main__":
    main()
