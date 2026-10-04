#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/clip_check.py -- the system clipboard of the window layer against GTK 3's.
#
#   usage: clip_check.py <clip_main binary> [wine]
#
# Both directions, on a real X server (Xvfb): the program owns the clipboard and GTK reads it; GTK owns it
# and the program reads it. With `wine` the program is the Windows build (CF_UNICODETEXT, CF_HDROP and a
# registered format) and Wine's clipboard bridge to the X selections stands between it and GTK.
import os, shutil, subprocess, sys, tempfile, time

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

for tool in ("Xvfb",):
    if shutil.which(tool) is None:
        print("  SKIP  %s not installed" % tool)
        sys.exit(0)
binary = sys.argv[1]
WINE = len(sys.argv) > 2 and sys.argv[2] == "wine"
here = os.path.dirname(os.path.abspath(__file__))
td = tempfile.mkdtemp(prefix="clip-check-")

def free_display():
    for n in range(250, 300):
        if not os.path.exists("/tmp/.X11-unix/X%d" % n) and not os.path.exists("/tmp/.X%d-lock" % n):
            return n
n = free_display()
xvfb = subprocess.Popen(["Xvfb", ":%d" % n, "-screen", "0", "800x600x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
env = dict(os.environ, DISPLAY=":%d" % n)
if WINE:
    env["WINEPREFIX"] = os.environ.get("WINEPREFIX", os.path.expanduser("~/.wine-firn"))
    env["WINEDEBUG"] = "-all"
for _ in range(100):
    if os.path.exists("/tmp/.X11-unix/X%d" % n):
        break
    time.sleep(0.1)
time.sleep(0.3)
procs = [xvfb]

def prog(*args, out):
    cmd = (["wine", "explorer", "/desktop=firn,800x600"] if WINE else []) + [binary] + list(args)
    f = open(out, "wb")
    p = subprocess.Popen(cmd, stdout=f, stderr=subprocess.DEVNULL, env=env)
    procs.append(p)
    return p

def wait_for(path, text, timeout=60):
    end = time.time() + timeout
    while time.time() < end:
        if os.path.exists(path) and text.encode() in open(path, "rb").read():
            return True
        time.sleep(0.2)
    return False

TEXT = "héllo wörld 日本語\nline two\n\nlast line"
try:
    for name in ("clip one.txt", "\u00fcber.txt"):
        open(os.path.join("/tmp", name), "w").write("x")     # the file list names files that exist
    tf = os.path.join(td, "text.txt")
    open(tf, "w", encoding="utf-8").write(TEXT)
    hostfile = ("Z:" + tf.replace("/", "\\")) if WINE else tf
    uf = os.path.join(td, "uris.txt")
    open(uf, "w", encoding="utf-8", newline="").write(
        ("file:///Z:/tmp/clip%20one.txt\r\nfile:///Z:/tmp/%C3%BCber.txt\r\n") if WINE else
        ("file:///tmp/clip%20one.txt\r\nfile:///tmp/%C3%BCber.txt\r\n"))
    hosturis = ("Z:" + uf.replace("/", "\\")) if WINE else uf
    # --- 1. the program owns the clipboard, GTK reads it
    o1 = os.path.join(td, "o1")
    p1 = prog("set", hostfile, hosturis, out=o1)
    check("the program set text, a file list and a private type", wait_for(o1, "READY"), open(o1, "rb").read() if os.path.exists(o1) else b"")
    out1 = open(o1, "rb").read().decode("utf-8", "replace")
    check("text set ok", "SET text ok" in out1, out1)
    check("file list set ok", "SET uris ok" in out1, out1)
    check("private type set ok", "SET private ok" in out1, out1)
    time.sleep(1.0)
    g = subprocess.run([sys.executable, os.path.join(here, "gtk_clip.py"), "get"], capture_output=True, text=True, env=env, timeout=60)
    lines = {l.split(" ", 1)[0]: l.split(" ", 1)[1] for l in g.stdout.splitlines() if " " in l}
    check("GTK reads the program's text exactly (UTF-8, line breaks)", lines.get("TEXT") == repr(TEXT), g.stdout + g.stderr)
    uris = eval(lines.get("URIS", "None")) if lines.get("URIS") else None
    if WINE:
        # CF_HDROP becomes a Windows path list; Wine offers it to X as text/uri-list with the Z: drive mapped back
        check("GTK sees a file list (two files)", uris is not None and len(uris) == 2, g.stdout)
    else:
        import urllib.parse
        check("GTK sees the two file URIs", uris is not None and [urllib.parse.unquote(u) for u in uris] == ["file:///tmp/clip one.txt", "file:///tmp/\u00fcber.txt"], uris)
    targets = eval(lines.get("TARGETS", "None")) if lines.get("TARGETS") else []
    check("GTK sees a text target", any(t in targets for t in ("UTF8_STRING", "text/plain;charset=utf-8", "text/plain", "STRING")), targets)
    if not WINE:
        check("GTK sees the private target", "application/x-firn-test" in targets, targets)
    p1.terminate()
    if WINE:
        subprocess.run(["wineserver", "-k"], env=env)      # the first program must not keep the Wine clipboard
    time.sleep(1.5)
    # --- 2. GTK owns the clipboard, the program reads it
    tf2 = os.path.join(td, "text2.txt")
    TEXT2 = "grüße буквы\nsecond line\r\nthird"   # one CRLF inside: stays as it is on X
    open(tf2, "w", encoding="utf-8", newline="").write(TEXT2)
    gp = subprocess.Popen([sys.executable, os.path.join(here, "gtk_clip.py"), "set", tf2], stdout=subprocess.PIPE, text=True, env=env)
    procs.append(gp)
    check("GTK owns the clipboard", gp.stdout.readline().strip() == "READY")
    time.sleep(0.5)
    o2 = os.path.join(td, "o2")
    p2 = prog("get", "text/plain;charset=utf-8", out=o2)
    check("the program read GTK's text", wait_for(o2, "GOT "), open(o2, "rb").read() if os.path.exists(o2) else b"")
    p2.wait(timeout=60)
    data = open(o2, "rb").read().decode("utf-8", "replace")
    got_text = data.split("\n", 1)[1].rstrip("\n") if data.startswith("GOT ") else None
    want = TEXT2.replace("\r\n", "\n")        # the window layer's text has "\n" line breaks on every system
    check("the text arrives with \\n line breaks and UTF-8 intact", got_text == want, data[:200])
    o3 = os.path.join(td, "o3")
    p3 = prog("types", out=o3)
    p3.wait(timeout=60)
    t3 = open(o3, "rb").read().decode("utf-8", "replace")
    check("the program lists a text type", "text/plain" in t3, t3)
    o4 = os.path.join(td, "o4")
    p4 = prog("get", "application/x-nothing-like-this", out=o4)
    p4.wait(timeout=60)
    check("a type nobody offers is NONE", "NONE" in open(o4, "rb").read().decode("utf-8", "replace"))
    gp.terminate()
finally:
    for p in procs[::-1]:
        try:
            p.terminate()
        except Exception:
            pass
    subprocess.run(["pkill", "-x", "wineserver"]) if WINE else None
    for name in ("clip one.txt", "\u00fcber.txt"):
        try:
            os.unlink(os.path.join("/tmp", name))
        except OSError:
            pass
    shutil.rmtree(td, ignore_errors=True)
print("clipboard: %d failed" % len(FAILED) if FAILED else "clipboard: all checks passed")
sys.exit(1 if FAILED else 0)
