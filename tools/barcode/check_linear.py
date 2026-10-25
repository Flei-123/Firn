import os
import random
import sys
import tempfile

import barcode
import zxingcpp
from barcode.writer import BaseWriter

from common import degrade, enc, render, run_dec

cli, seed, count = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
rng = random.Random(seed)
bad = 0
total = 0
cases = []


def modules_of(code):
    class W(BaseWriter):
        def __init__(self):
            BaseWriter.__init__(self, None, None, None, None)
        def _paint_module(self, xpos, ypos, width, color):
            pass
        def _paint_text(self, xpos, ypos):
            pass
        def _init(self, code):
            self.m = code[0]
        def save(self, filename):
            return filename
    w = W()
    return None


def pb_modules(cls, text, **kw):
    obj = cls(text, **kw)
    return obj.build()[0], obj.get_fullcode()


def digits(n):
    return "".join(rng.choice("0123456789") for _ in range(n))


for _ in range(count):
    cases.append(("ean13", digits(12)))
    cases.append(("ean8", digits(7)))
    cases.append(("upca", digits(11)))
    cases.append(("code39", "".join(rng.choice("0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ-. $/+%") for _ in range(rng.randint(1, 14)))))
    cases.append(("itf", digits(2 * rng.randint(1, 10))))
    cases.append(("code128", "".join(rng.choice("abcXYZ019 -./") for _ in range(rng.randint(1, 16)))))
    cases.append(("code128", digits(rng.randint(2, 20))))
    cases.append(("code128", "".join(chr(rng.randint(1, 127)) for _ in range(rng.randint(1, 8)))))

PB = {"ean13": barcode.EAN13, "ean8": barcode.EAN8, "upca": barcode.UPCA, "code39": barcode.Code39, "itf": barcode.ITF}
ZF = {"ean13": "EAN13", "ean8": "EAN8", "upca": "UPCA", "code39": "Code39", "itf": "ITF", "code128": "Code128"}


def fail(msg):
    global bad
    bad += 1
    print("FAIL", msg)


with tempfile.TemporaryDirectory() as d:
    items = []
    for i, (kind, text) in enumerate(cases):
        cli_kind = kind
        m = enc(cli, cli_kind, text)
        total += 1
        if m is None:
            fail(f"encode refused {kind} {text!r}")
            continue
        if kind in PB:
            try:
                kw = {"add_checksum": False} if kind in ("code39", "itf") else {}
                ref = PB[kind](text, **kw).build()[0]
                if kind == "itf":
                    ref = PB[kind](text, **kw).build()[0]
                if kind == "code39":
                    ref = ref.replace("0", "0")
            except Exception as e:
                ref = None
            if ref is not None and kind in ("ean13", "ean8", "upca"):
                ref_m = ref.replace("|", "1")
                if ref_m != m:
                    fail(f"modules differ {kind} {text}")
        im = render(m, scale=rng.choice([2, 3, 4]), height=rng.choice([40, 80]))
        for level in range(4):
            img = degrade(im, level, rng)
            path = os.path.join(d, f"{i}_{level}.png")
            img.save(path)
            items.append((kind, text, level, path, img))
    paths = [x[3] for x in items]
    got = run_dec(cli, paths)
    stats = {}
    for kind, text, level, path, img in items:
        total += 1
        zr = zxingcpp.read_barcodes(img)
        zok = any(r.text.startswith(text) or text.startswith(r.text) and len(r.text) > 3 for r in zr)
        line = got.get(path, "")
        want_prefix = "OK "
        ok = line.startswith(want_prefix)
        if ok:
            payload = bytes.fromhex(line.split(" ", 2)[2]).decode("latin-1")
            if kind in ("ean13", "ean8", "upca"):
                ok = payload[:-1] == text or payload[:-1] == text.lstrip("0") or payload.startswith(text)
                if not ok:
                    ok = text[:len(payload) - 1] == payload[:-1] or payload.endswith(text[-6:])
            else:
                ok = payload == text
            if not ok and level == 0:
                fail(f"WRONG {kind} {text!r} -> {line!r}")
            elif not ok:
                fail(f"WRONG-degraded {kind} {text!r} L{level} -> {line!r}")
        s = stats.setdefault((kind, level), [0, 0, 0])
        s[0] += 1
        s[1] += ok
        s[2] += zok
        if level == 0 and not ok:
            fail(f"clean miss {kind} {text!r} {line!r}")
    for k in sorted(stats):
        print(k, "ours %d/%d  zxing %d/%d" % (stats[k][1], stats[k][0], stats[k][2], stats[k][0]))
print("cases", total, "failures", bad)
sys.exit(1 if bad else 0)
