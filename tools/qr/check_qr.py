#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/qr/check_qr.py -- holds lib/qr (the Firn QR encoder) against four
# implementations nobody here wrote:
#   * qrcodegen (Project Nayuki's reference encoder): the module matrix must be
#     IDENTICAL, module for module, for the same data, level, version range, mask
#     (forced or automatic) and level boost;
#   * python-qrcode: identical for a forced version AND a forced mask (it picks
#     the automatic mask by rules of its own, so that is not compared);
#   * ZXing-C++ (zxingcpp): every code must read back to the octets that went in;
#   * segno: only REPORTED -- segno writes a zero octet instead of the pad
#     codeword 0xEC when exactly one pad codeword is needed in byte mode
#     (ISO/IEC 18004 7.4.10 says 0xEC; qrcodegen, python-qrcode and we write it;
#     no reader looks behind the terminator), and its automatic mask differs.
# usage: check_qr.py <qrcli binary> [seed] [cases] [-v]
# needs: qrcodegen, qrcode, segno, zxing-cpp, Pillow (PYTHONPATH may point at a
# pip --target directory)
import random
import subprocess
import sys

import qrcode
import segno
import qrcodegen
from qrcode import constants as qc
from qrcode.util import QRData
import zxingcpp
from PIL import Image

cli = sys.argv[1]
seed = int(sys.argv[2]) if len(sys.argv) > 2 else 1
ncases = int(sys.argv[3]) if len(sys.argv) > 3 else 600
rnd = random.Random(seed)

LEVELS = "LMQH"
ALNUM = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:"
SEGNO_MODE = {1: "numeric", 2: "alphanumeric", 4: "byte"}


def make_data(mode, maxlen):
    n = rnd.randint(1, maxlen)
    if mode == 1:
        return "".join(rnd.choice("0123456789") for _ in range(n)).encode()
    if mode == 2:
        return "".join(rnd.choice(ALNUM) for _ in range(n)).encode()
    return bytes(rnd.randrange(256) for _ in range(n))


cases = []
for i in range(ncases):
    mode = rnd.choice([1, 2, 4])
    ecc = rnd.randrange(4)
    sel = rnd.random()
    if sel < 0.35:
        # a forced version, short data that must fit
        ver = rnd.randint(1, 40)
        maxlen = {1: 10, 2: 6, 4: 4}[mode] + ver * {1: 5, 2: 3, 4: 2}[mode]
        data = make_data(mode, max(1, maxlen // 2))
        minv = maxv = ver
    elif sel < 0.9:
        data = make_data(mode, rnd.choice([5, 30, 120, 400]))
        minv, maxv = 1, 40
    else:
        data = make_data(mode, {1: 3000, 2: 1800, 4: 1200}[mode])
        minv, maxv = 1, 40
    mask = rnd.choice([-1, -1, 0, 1, 2, 3, 4, 5, 6, 7])
    boost = 1 if rnd.random() < 0.25 else 0
    cases.append((mode, ecc, minv, maxv, mask, data, boost))

for mask in range(8):
    cases.append((4, mask % 4, 1, 40, mask, b"mask check " + bytes([mask]), 0))
for ver in range(1, 41):
    for ecc in range(4):
        cases.append((4, ecc, ver, ver, -1, bytes(rnd.randrange(256) for _ in range(3 + ver)), 0))

inp = "".join(
    "%d %d %d %d %d %d %s\n" % (m, e, a, b, k, bo, d.hex() or "-") for (m, e, a, b, k, d, bo) in cases
)
res = subprocess.run([cli], input=inp.encode(), capture_output=True, check=True).stdout.decode().splitlines()
assert len(res) == len(cases), (len(res), len(cases))

QMODE = {1: 1, 2: 2, 4: 4}
QLEVEL = [qc.ERROR_CORRECT_L, qc.ERROR_CORRECT_M, qc.ERROR_CORRECT_Q, qc.ERROR_CORRECT_H]
NAYUKI = [qrcodegen.QrCode.Ecc.LOW, qrcodegen.QrCode.Ecc.MEDIUM,
          qrcodegen.QrCode.Ecc.QUARTILE, qrcodegen.QrCode.Ecc.HIGH]


def nayuki_matrix(m, e, a, b, k, d, bo):
    if m == 1:
        seg = qrcodegen.QrSegment.make_numeric(d.decode())
    elif m == 2:
        seg = qrcodegen.QrSegment.make_alphanumeric(d.decode())
    else:
        seg = qrcodegen.QrSegment.make_bytes(d)
    try:
        q = qrcodegen.QrCode.encode_segments([seg], NAYUKI[e], a, b, k, bool(bo))
    except Exception:
        return None, 0, 0
    n = q.get_size()
    return "".join("1" if q.get_module(x, y) else "0" for y in range(n) for x in range(n)), q.get_version(), q.get_mask()


def qrcode_matrix(m, e, a, b, k, d):
    if a != b or k < 0:
        return None
    try:
        q = qrcode.QRCode(version=a, error_correction=QLEVEL[e], mask_pattern=k, border=0)
        q.add_data(QRData(d, mode=QMODE[m]), optimize=0)
        q.make(fit=False)
    except Exception:
        return None
    return "".join("1" if c else "0" for r in q.modules for c in r)

def segno_matrix(m, e, a, b, k, d):
    kw = dict(error=LEVELS[e], mode=SEGNO_MODE[m], micro=False, boost_error=False)
    if a == b:
        kw["version"] = a
    if k >= 0:
        kw["mask"] = k
    try:
        sq = segno.make(d.decode() if m != 4 else d, **kw)
    except Exception:
        return None
    return "".join("1" if c else "0" for r in sq.matrix for c in r)

bad = 0
same = 0
read_ok = 0
segno_same = 0
segno_total = 0
qc_same = 0
qc_total = 0
for (m, e, a, b, k, d, bo), line in zip(cases, res):
    f = line.split()
    nm, nv, nk = nayuki_matrix(m, e, a, b, k, d, bo)
    if f[0] == "ERR":
        if nm is not None:
            print("FAIL ours refused (%s) but qrcodegen encodes: mode=%d ecc=%d v=%d..%d len=%d" % (f[1], m, e, a, b, len(d)))
            bad += 1
        else:
            same += 1
        continue
    version, ecc_o, mask_o, mode_o, size = (int(x) for x in f[1:6])
    bits = f[6]
    if nm is None:
        print("FAIL qrcodegen refuses but ours encodes: mode=%d ecc=%d v=%d..%d len=%d" % (m, e, a, b, len(d)))
        bad += 1
        continue
    if nv != version or nm != bits or nk != mask_o:
        why = "version %s/%s" % (version, nv) if nv != version else ("mask %s/%s" % (mask_o, nk) if nk != mask_o else "modules")
        print("FAIL differs from qrcodegen (%s): mode=%d ecc=%d v=%d..%d mask=%d boost=%d len=%d" % (why, m, e, a, b, k, bo, len(d)))
        bad += 1
        continue
    same += 1
    qm = qrcode_matrix(m, e, a, b, k, d) if not bo else None
    if qm is not None:
        qc_total += 1
        if qm == bits:
            qc_same += 1
        else:
            print("FAIL differs from python-qrcode: mode=%d ecc=%d v=%d mask=%d len=%d" % (m, e, a, k, len(d)))
            bad += 1
    sm = segno_matrix(m, e, a, b, k, d) if not bo else None
    if sm is not None:
        segno_total += 1
        if sm == bits:
            segno_same += 1
    border = 4
    scale = 4
    side = (size + 2 * border) * scale
    img = Image.new("L", (side, side), 255)
    px = img.load()
    for y in range(size):
        for x in range(size):
            if bits[y * size + x] == "1":
                for dy in range(scale):
                    for dx in range(scale):
                        px[(x + border) * scale + dx, (y + border) * scale + dy] = 0
    out = zxingcpp.read_barcodes(img, formats=zxingcpp.BarcodeFormat.QRCode)
    if len(out) == 1 and out[0].bytes == d:
        read_ok += 1
    else:
        print("FAIL zxing read-back: mode=%d ecc=%d version=%d len=%d got=%r" % (
            m, e, version, len(d), [(str(o.format), len(o.bytes)) for o in out]))
        bad += 1

print("cases %d  identical to qrcodegen %d  identical to python-qrcode %d of %d  read back by zxing %d  "
      "(segno identical %d of %d, reported only)  failures %d" % (
          len(cases), same, qc_same, qc_total, read_ok, segno_same, segno_total, bad))
sys.exit(1 if bad else 0)
