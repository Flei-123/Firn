"""testkit.py -- the shared test helpers for Python test scripts (import it).

    import os, sys
    sys.path.insert(0, os.path.join(os.environ.get("FIRN", "/root/firn"), "tools/testkit"))
    from testkit import Kit, read_ppm, ppm_diff

    k = Kit("my test")
    k.check("two plus two", 2 + 2 == 4)       # ok / bad with the same lines as testkit.sh
    w, h, rgb = read_ppm("shot.ppm")           # P6 PPM, comments in the header allowed
    k.check("same picture", ppm_diff(a, b)[0] == 0)
    sys.exit(k.summary())

Kit.ok / Kit.bad print "  OK    label" / "  FAIL  label" (the lines testkit.sh prints) and count.
Picture helpers: read_ppm, parse_ppm (bytes), parse_ppm_np (numpy array), write_ppm, ppm_to_png (PNG without any package), ppm_diff, ppm_crop, ppm_solid.
"""
import os
import shutil
import struct
import tempfile
import zlib


class Kit:
    def __init__(self, name="tests"):
        self.name = name
        self.passed = 0
        self.failed = 0
        self.dirs = []

    def ok(self, label):
        self.passed += 1
        print("  OK    %s" % label)

    def bad(self, label):
        self.failed += 1
        print("  FAIL  %s" % label)

    def check(self, label, cond):
        (self.ok if cond else self.bad)(label)
        return bool(cond)

    def tmpdir(self, prefix="tk"):
        d = tempfile.mkdtemp(prefix=prefix + ".")
        self.dirs.append(d)
        return d

    def summary(self):
        for d in self.dirs:
            shutil.rmtree(d, ignore_errors=True)
        print("%s: %d passed, %d failed" % (self.name, self.passed, self.failed))
        return 0 if self.failed == 0 else 1


def parse_ppm(data):
    """P6 PPM bytes -> (width, height, rgb bytes). SystemExit on anything else."""
    if not data.startswith(b"P6"):
        raise SystemExit("testkit: not a P6 PPM")
    fields, i = [], 2
    while len(fields) < 3:
        while i < len(data) and data[i:i + 1].isspace():
            i += 1
        if data[i:i + 1] == b"#":
            while i < len(data) and data[i:i + 1] != b"\n":
                i += 1
            continue
        j = i
        while j < len(data) and not data[j:j + 1].isspace():
            j += 1
        fields.append(int(data[i:j]))
        i = j
    i += 1
    w, h, _maxval = fields
    return w, h, data[i:i + w * h * 3]


def parse_ppm_np(data):
    """P6 PPM bytes -> numpy uint8 array of shape (h, w, 3) (numpy is imported here, not at load)."""
    import numpy as np
    w, h, rgb = parse_ppm(data)
    return np.frombuffer(rgb, dtype=np.uint8).reshape(h, w, 3)


def read_ppm(path):
    with open(path, "rb") as f:
        return parse_ppm(f.read())


def write_ppm(path, w, h, rgb):
    with open(path, "wb") as f:
        f.write(b"P6\n%d %d\n255\n" % (w, h) + bytes(rgb))


def ppm_solid(w, h, rgb):
    return w, h, bytes(rgb) * (w * h)


def ppm_crop(img, x, y, cw, ch):
    w, h, d = img
    rows = [d[((y + r) * w + x) * 3:((y + r) * w + x + cw) * 3] for r in range(ch)]
    return cw, ch, b"".join(rows)


def ppm_diff(a, b, tol=0):
    """(pixels that differ by more than tol in any channel, largest channel delta). Sizes must match."""
    if a[0] != b[0] or a[1] != b[1]:
        raise ValueError("size differs: %dx%d vs %dx%d" % (a[0], a[1], b[0], b[1]))
    da, db = a[2], b[2]
    n = worst = 0
    for i in range(0, len(da), 3):
        m = max(abs(da[i] - db[i]), abs(da[i + 1] - db[i + 1]), abs(da[i + 2] - db[i + 2]))
        if m > tol:
            n += 1
        if m > worst:
            worst = m
    return n, worst


def ppm_to_png(img, path):
    w, h, d = img
    raw = b"".join(b"\0" + d[y * w * 3:(y + 1) * w * 3] for y in range(h))

    def chunk(t, body):
        c = struct.pack(">I", len(body)) + t + body
        return c + struct.pack(">I", zlib.crc32(t + body) & 0xffffffff)

    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw, 6)) + chunk(b"IEND", b""))
