#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_ipp.py <ipp_probe> <workdir> -- lib/print IPP client and raster encoders
# against ippeveprinter (a real IPP Everywhere printer) and independent Python decoders of PWG raster / URF.
import os, shutil, socket, struct, subprocess, sys, time
probe, work = sys.argv[1], sys.argv[2]
bad = 0
def fail(m):
    global bad
    bad += 1
    print("  FAIL", m)
def ok(m):
    print("  ok  ", m)
def run(*a):
    return subprocess.run([probe] + [str(x) for x in a], capture_output=True, text=True, timeout=60).stdout.strip()
def free_port():
    s = socket.socket(); s.bind(("127.0.0.1", 0)); p = s.getsockname()[1]; s.close(); return p

def unpack(data, bpl, unit, h):
    i, rows = 0, []
    while len(rows) < h:
        rep = data[i] + 1; i += 1
        line = bytearray()
        while len(line) < bpl:
            c = data[i]; i += 1
            if c == 128:
                line += b"\xff" * (bpl - len(line))
            elif c < 128:
                line += data[i:i + unit] * (c + 1); i += unit
            else:
                n = 257 - c; line += data[i:i + n * unit]; i += n * unit
        if len(line) != bpl:
            raise ValueError("line length")
        rows += [bytes(line)] * rep
    return rows[:h], i

def expect_pixel(n, x, y, h, fmt):
    if y < h // 2:
        return (255, 255, 255)
    return ((x * 7 + y) & 255, (x * 3 + n) & 255, (y * 5) & 255)

def luma(p):
    return (p[0] * 77 + p[1] * 150 + p[2] * 29) >> 8

for fmt, name in ((2, "srgb"), (1, "gray")):
    for kind in ("pwg", "urf"):
        f = os.path.join(work, "t_%s_%s.bin" % (kind, name))
        r = run("raster", kind, f, 150, fmt, 200, 100)
        d = open(f, "rb").read()
        w, h = 417, 208
        w, h = (200 * 150 + 36) // 72, (100 * 150 + 36) // 72
        bpp = 24 if fmt == 2 else 8
        unit = bpp // 8
        pos = 4 if kind == "pwg" else 12
        good = r == "1"
        if kind == "pwg":
            good = good and d[:4] == b"RaS2"
        else:
            good = good and d[:8] == b"UNIRAST\0" and struct.unpack(">I", d[8:12])[0] == 2
        try:
            for n in (0, 1):
                if kind == "pwg":
                    hdr = d[pos:pos + 1796]; pos += 1796
                    cw, ch, cbpp, bpl, cs = struct.unpack(">IIII", hdr[372:388])[0], struct.unpack(">I", hdr[376:380])[0], struct.unpack(">I", hdr[388:392])[0], struct.unpack(">I", hdr[392:396])[0], struct.unpack(">I", hdr[400:404])[0]
                    dpix = struct.unpack(">II", hdr[276:284])
                    pw = struct.unpack(">II", hdr[352:360])
                    good = good and hdr[:9] == b"PwgRaster" and cw == w and ch == h and cbpp == bpp and bpl == w * unit and dpix == (150, 150) and pw == (200, 100)
                    good = good and cs == (19 if fmt == 2 else 18) and hdr[1732:1748] == b"iso_a4_210x297mm"
                else:
                    hdr = d[pos:pos + 32]; pos += 32
                    good = good and hdr[0] == bpp and struct.unpack(">III", hdr[12:24]) == (w, h, 150)
                rows, used = unpack(d[pos:], w * unit, unit, h); pos += used
                for (x, y) in ((0, 0), (5, h - 3), (w - 1, h - 1), (17, h // 2 + 1)):
                    e = expect_pixel(n, x, y, h, fmt)
                    px = rows[y][x * unit:(x + 1) * unit]
                    got = tuple(px) if fmt == 2 else (px[0],)
                    want = e if fmt == 2 else (luma(e),)
                    if got != want:
                        good = False
            good = good and pos == len(d)
        except Exception as ex:
            good = False
            print("   ", ex)
        (ok if good else fail)("%s %s: header, run-length lines and pixels decode with an independent decoder (%d octets)" % (kind, name, len(d)))

if shutil.which("ippeveprinter"):
    spool = os.path.join(work, "spool"); os.makedirs(spool, exist_ok=True)
    port = free_port()
    ipp = subprocess.Popen(["ippeveprinter", "-p", str(port), "-f", "application/pdf,image/pwg-raster,image/urf", "-k", "-d", spool,
                            "-r", "off", "-2", "-l", "Shop", "-M", "Firn", "-m", "Model", "FirnIpp"],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    uri = "ipp://127.0.0.1:%d/ipp/print" % port
    for _ in range(50):
        try:
            socket.create_connection(("127.0.0.1", port), 0.2).close(); break
        except OSError:
            time.sleep(0.1)
    try:
        a = run("attrs", uri).split("|")
        if a[0] == "1" and a[1] == "FirnIpp" and a[2] == "3" and "image/pwg-raster" in a[3] and "iso_a4_210x297mm" in a[4] and "two-sided-long-edge" in a[5] and int(a[7]) > 0:
            ok("Get-Printer-Attributes against ippeveprinter: name, state, formats, media, sides, media-size collections (%s)" % a[7])
        else:
            fail("attrs %r" % a)
        f = os.path.join(work, "t_pwg_srgb.bin")
        r = run("print", uri, f, "image/pwg-raster", 2, "two-sided-long-edge", "1-2").splitlines()
        if r and r[0].startswith("JOB ") and r[0].split()[2] == "0" and r[1].startswith("STATE ") and r[1].split()[1] in ("3", "5", "7", "9"):
            ok("Print-Job (copies, sides, page-ranges, pwg raster) + Get-Job-Attributes: %s / %s" % (r[0], r[1]))
        else:
            fail("print %r" % r)
        if len(r) > 2 and r[2].startswith("CANCEL"):
            ok("Cancel-Job answered: %s" % r[2])
        else:
            fail("cancel %r" % r)
        r = run("print", uri, f, "image/unsupported-format", 1, "", "").splitlines()
        if r and (r[0].startswith("JOB 0 1034") or r[0].startswith("JOB 0 1035")):
            ok("an unsupported format is refused by the printer (IPP 0x040A or 0x040B)")
        else:
            fail("unsupported %r" % r)
    finally:
        ipp.terminate(); ipp.wait()
else:
    print("  skip: ippeveprinter not installed")
r = run("attrs", "ipp://127.0.0.1:%d/ipp/print" % free_port()).split("|")
(ok if r[0] == "0" else fail)("nothing listening: failure reported")
print("FAILED %d" % bad if bad else "ALL OK")
sys.exit(1 if bad else 0)
