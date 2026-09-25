#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_clip.py <clip_probe> -- the X11 clipboard of
# lib/window (OpenPlan LIB-006) against Tk and python-xlib on an Xvfb of
# its own: Firn owns and others read, others own and Firn reads, TARGETS,
# a program's own MIME type, losing the ownership (SelectionClear).
import os, shutil, subprocess, sys, time
probe = sys.argv[1]
if not shutil.which("Xvfb"):
    print("  skip: no Xvfb"); sys.exit(0)
try:
    import tkinter
except ImportError:
    print("  skip: no tkinter"); sys.exit(0)
disp = None
for n in range(140, 160):
    if not os.path.exists("/tmp/.X11-unix/X%d" % n):
        disp = n; break
xvfb = subprocess.Popen(["Xvfb", ":%d" % disp, "-screen", "0", "640x480x24", "-nolisten", "tcp"],
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
env = dict(os.environ, DISPLAY=":%d" % disp)
os.environ["DISPLAY"] = ":%d" % disp
for _ in range(50):
    if os.path.exists("/tmp/.X11-unix/X%d" % disp): break
    time.sleep(0.1)
time.sleep(0.3)
bad = 0
def fail(m):
    global bad
    bad += 1
    print("  FAIL", m)
def ok(m):
    print("  ok  ", m)
def firn(*a, wait=True):
    if wait:
        return subprocess.run([probe] + list(a), env=env, capture_output=True, text=True, timeout=30).stdout.rstrip("\n")
    return subprocess.Popen([probe] + list(a), env=env, stdout=subprocess.PIPE, text=True)
def tk_get(typ=None):
    code = "import tkinter\nr=tkinter.Tk()\ntry:\n  print(repr(r.clipboard_get(%s)))\nexcept Exception as e:\n  print('ERR')\n" % ("type=%r" % typ if typ else "")
    return subprocess.run([sys.executable, "-c", code], env=env, capture_output=True, text=True, timeout=30).stdout.strip()
def tk_own(text, secs, from_file=None):
    what = "open(%r).read()" % from_file if from_file else repr(text)
    code = "import tkinter\nr=tkinter.Tk()\nr.clipboard_clear()\nr.clipboard_append(%s)\nr.update()\nr.after(%d, r.destroy)\nr.mainloop()\n" % (what, secs * 1000)
    t = subprocess.Popen([sys.executable, "-c", code.replace("r.update()\n", "r.update()\nprint('OWNED', flush=True)\n")],
                         env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    t.stdout.readline()  # Tk owns the clipboard now
    return t
try:
    text = "Schütz -K1 ✓ Größe"
    js = '{"devices":["-K1","-K2"]}'
    # --- nobody owns the clipboard
    r = firn("get", "text/plain")
    (ok if r == "NONE" else fail)("no owner: nothing to get (%s)" % r)
    # --- Firn owns text and its own type
    p = firn("own", "5", "text/plain;charset=utf-8", text, "application/x-openplan+json", js, wait=False)
    assert p.stdout.readline().strip() == "OWNING"
    r = firn("get", "text/plain")
    (ok if r == text else fail)("Firn -> Firn: text (%r)" % r)
    r = firn("get", "application/x-openplan+json")
    (ok if r == js else fail)("Firn -> Firn: application/x-openplan+json (%r)" % r)
    r = firn("types").split("\n")
    want = ["UTF8_STRING", "STRING", "TEXT", "text/plain;charset=utf-8", "application/x-openplan+json"]
    (ok if r == want else fail)("TARGETS as another program sees them: %s" % ", ".join(r))
    r = tk_get()
    (ok if r == repr(text) else fail)("Firn -> Tk: text with umlauts and a check mark (%s)" % r)
    r = tk_get("application/x-openplan+json")
    decoded = bytes(int(h, 16) for h in r.strip("'").split()).decode() if r.startswith("'0x") else r
    (ok if decoded == js else fail)("Firn -> Tk: the program's own type, octet for octet (%s)" % decoded)
    try:
        from Xlib import display, X, Xatom
        d = display.Display(":%d" % disp)
        w = d.screen().root.create_window(0, 0, 1, 1, 0, X.CopyFromParent)
        clip, targets, prop = d.intern_atom("CLIPBOARD"), d.intern_atom("TARGETS"), d.intern_atom("XPROBE")
        w.convert_selection(clip, targets, prop, X.CurrentTime)
        names = None
        t0 = time.time()
        while time.time() - t0 < 3:
            if d.pending_events():
                e = d.next_event()
                if e.type == X.SelectionNotify:
                    v = w.get_full_property(prop, X.AnyPropertyType)
                    names = [d.get_atom_name(a) for a in v.value] if v else None
                    break
            else:
                time.sleep(0.05)
        (ok if names and names[0] == "TARGETS" and "UTF8_STRING" in names else fail)("python-xlib: TARGETS answered with type ATOM (%s)" % names)
    except ImportError:
        print("  skip: python-xlib")
    # --- Tk takes the clipboard over: Firn loses it (SelectionClear) and
    # then reads Tk's text like everybody else
    t = tk_own("Tk: Klemme X1:5", 6)
    out = p.communicate(timeout=20)[0].strip()
    (ok if out == "Tk: Klemme X1:5" else fail)("SelectionClear: the former owner now reads the new owner's text (%r)" % out)
    r = firn("get", "text/plain")
    (ok if r == "Tk: Klemme X1:5" else fail)("Tk -> Firn: text (%r)" % r)
    r = firn("types").split("\n")
    (ok if "UTF8_STRING" in r else fail)("Tk's TARGETS read by Firn (%s)" % " ".join(r))
    r = firn("get", "application/x-openplan+json")
    (ok if r == "NONE" else fail)("a type the owner does not have: refused (%s)" % r)
    t.wait()
    # --- large contents go as INCR transfers (ICCCM 2.7.2), both ways
    import random, tempfile
    rng = random.Random(3)
    big = "".join(rng.choice("abcdeäöü€ \n") for _ in range(700000))
    blob = bytes(rng.getrandbits(8) for _ in range(1500000))
    tmp = tempfile.mkdtemp()
    bigf, blobf, outf = os.path.join(tmp, "big.txt"), os.path.join(tmp, "blob.png"), os.path.join(tmp, "out")
    open(bigf, "w").write(big)
    open(blobf, "wb").write(blob)
    p = firn("ownfile", "6", "text/plain", bigf, wait=False)
    assert p.stdout.readline().strip() == "OWNING"
    r = tk_get()
    (ok if r == repr(big) else fail)("Firn -> Tk: %d octets of text through INCR" % len(big.encode()))
    r = firn("getfile", "text/plain", outf)
    (ok if r == "GOT" and open(outf, "rb").read() == big.encode() else fail)("Firn -> Firn: the same through INCR")
    p.communicate(timeout=20)
    p = firn("ownfile", "4", "image/png", blobf, wait=False)
    assert p.stdout.readline().strip() == "OWNING"
    r = firn("getfile", "image/png", outf)
    (ok if r == "GOT" and open(outf, "rb").read() == blob else fail)("Firn -> Firn: %d octets of binary data (image/png) through INCR" % len(blob))
    p.communicate(timeout=20)
    t = tk_own(None, 5, from_file=bigf)
    r = firn("getfile", "text/plain", outf)
    (ok if r == "GOT" and open(outf, "rb").read() == big.encode() else fail)("Tk -> Firn: %d octets through INCR" % len(big.encode()))
    t.wait()
finally:
    xvfb.terminate()
print("clipboard: %d failed" % bad)
sys.exit(1 if bad else 0)
