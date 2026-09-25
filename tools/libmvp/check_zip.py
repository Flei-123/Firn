#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_zip.py <zip_probe> <workdir> -- lib/zip against Python's
# zipfile and Info-ZIP's unzip, in both directions.
import io, os, subprocess, sys, zipfile, zlib, warnings
probe, work = sys.argv[1], sys.argv[2]
bad = 0
def fail(msg):
    global bad
    bad += 1
    print("  FAIL", msg)
def ok(msg):
    print("  ok  ", msg)

def firn_read(path):
    out = subprocess.run([probe, "read", path], capture_output=True, check=True).stdout.decode()
    return out.splitlines()

def python_view(path):
    z = zipfile.ZipFile(path)
    rows = []
    for i in z.infolist():
        data = z.read(i)
        rows.append("%s\t%d\t%08x" % (i.filename, len(data), zlib.crc32(data)))
    return rows

# 1. what Firn writes, others read
for mode, name in (("write", "firn.zip"), ("write64", "firn64.zip")):
    p = os.path.join(work, name)
    subprocess.run([probe, mode, p], check=True)
    u = subprocess.run(["unzip", "-tq", p], capture_output=True, text=True)
    if u.returncode == 0: ok("unzip -t accepts %s" % name)
    else: fail("unzip -t %s: %s" % (name, u.stdout[-300:]))
    z = zipfile.ZipFile(p)
    if z.testzip() is None: ok("zipfile.testzip accepts %s (%d entries)" % (name, len(z.namelist())))
    else: fail("zipfile.testzip %s" % name)
    if firn_read(p) == python_view(p): ok("Firn and Python extract %s alike" % name)
    else: fail("Firn and Python differ on %s" % name)

# 2. what Python writes, Firn reads
def case(name, build, expect_refusal=None, expect_errors=0):
    p = os.path.join(work, name)
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        build(p)
    got = firn_read(p)
    if expect_refusal:
        if got == ["REFUSED " + expect_refusal]: ok("%s refused: %s" % (name, expect_refusal))
        else: fail("%s: expected REFUSED %s, got %s" % (name, expect_refusal, got[:3]))
        return
    errors = [g for g in got if "\tERROR " in g]
    if expect_errors:
        if len(errors) == expect_errors: ok("%s: %d entries refused as expected (%s)" % (name, len(errors), errors[0].split("\t")[1]))
        else: fail("%s: expected %d refused entries, got %s" % (name, expect_errors, got))
        return
    want = python_view(p)
    if got == want: ok("%s: %d entries identical" % (name, len(want)))
    else: fail("%s: firn %s python %s" % (name, got[:4], want[:4]))

payload = (b"Wendeschuetz K1 K2 " * 5000) + os.urandom(3000)
def deflated(p):
    with zipfile.ZipFile(p, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        z.writestr("a.txt", payload); z.writestr("dir/", b""); z.writestr("dir/b.bin", os.urandom(10000))
        z.writestr("leer", b""); z.writestr("Ärger/ü.txt", "grüß".encode()); z.comment = b"a comment at the end"
def stored(p):
    with zipfile.ZipFile(p, "w", zipfile.ZIP_STORED) as z:
        for i in range(50): z.writestr("s/%02d" % i, payload[: i * 37])
def streamed(p):
    # a non-seekable target: every entry carries a data descriptor (bit 3)
    raw = io.BytesIO()
    class NoSeek(io.RawIOBase):
        def writable(self): return True
        def write(self, b): return raw.write(b)
    with zipfile.ZipFile(NoSeek(), "w", zipfile.ZIP_DEFLATED) as z:
        for i in range(5):
            with z.open("stream/%d" % i, "w") as f: f.write(payload[: 1000 * (i + 1)])
    open(p, "wb").write(raw.getvalue())
def forced64(p):
    with zipfile.ZipFile(p, "w", zipfile.ZIP_DEFLATED) as z:
        for i in range(3):
            with z.open("big/%d" % i, "w", force_zip64=True) as f: f.write(payload)
def infozip(p):
    src = os.path.join(work, "src"); os.makedirs(os.path.join(src, "sub"), exist_ok=True)
    open(os.path.join(src, "one.txt"), "wb").write(payload)
    open(os.path.join(src, "sub", "two.bin"), "wb").write(os.urandom(5000))
    if os.path.exists(p): os.remove(p)
    subprocess.run(["zip", "-qr", os.path.abspath(p), "."], cwd=src, check=True)
def slip(p):
    with zipfile.ZipFile(p, "w") as z: z.writestr("ok.txt", b"x"); z.writestr("../../etc/evil", b"x")
def absolute(p):
    with zipfile.ZipFile(p, "w") as z: z.writestr(zipfile.ZipInfo("/tmp/evil"), b"x")
def backslash(p):
    with zipfile.ZipFile(p, "w") as z: z.writestr(zipfile.ZipInfo("a\\..\\..\\evil"), b"x")
def duplicate(p):
    with zipfile.ZipFile(p, "w") as z: z.writestr("same", b"1"); z.writestr("same", b"2")
def bzip(p):
    with zipfile.ZipFile(p, "w", zipfile.ZIP_BZIP2) as z: z.writestr("a", payload); z.writestr("b", payload)
def lzma(p):
    with zipfile.ZipFile(p, "w", zipfile.ZIP_LZMA) as z: z.writestr("a", payload)

case("py_deflated.zip", deflated)
case("py_stored.zip", stored)
case("py_streamed.zip", streamed)
case("py_zip64.zip", forced64)
case("infozip.zip", infozip)
case("slip.zip", slip, expect_refusal="UnsafeName")
case("absolute.zip", absolute, expect_refusal="UnsafeName")
case("backslash.zip", backslash, expect_refusal="UnsafeName")
case("duplicate.zip", duplicate, expect_refusal="Duplicate")
case("bzip2.zip", bzip, expect_errors=2)
case("lzma.zip", lzma, expect_errors=1)
print("zip: %d failed" % bad)
sys.exit(1 if bad else 0)
