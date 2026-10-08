#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/clipboard_check.py -- std.clipboard (lib/std/clipboard.fi) against GTK 3's clipboard.
#
#   usage: clipboard_check.py <clipboard_main binary> [wine]
#
# On a real X server (Xvfb), both directions, three kinds of content:
#   * the program owns the clipboard and GTK reads it: UTF-8 text with line ends, a 1.5 MiB text (an INCR transfer),
#     a PNG (GTK decodes it and the pixels are compared)
#   * GTK owns the clipboard and the program reads it: the same three
#   * `clear` takes the clipboard away from a foreign owner; `has` and `paste` agree with that
# With `wine` the program is the Windows build (CF_UNICODETEXT, the registered format "PNG") and Wine's clipboard
# bridge to the X selections stands between it and GTK. FIRN_CLIPBOARD=native is set for the Linux program: the
# helper programs (xclip ...) must not be what is being measured.
import hashlib, os, shutil, subprocess, sys, tempfile, time

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

if shutil.which("Xvfb") is None:
    print("  SKIP  Xvfb not installed")
    sys.exit(0)
try:
    import gi
except ImportError:
    print("  SKIP  python3 gi (GTK 3) not installed")
    sys.exit(0)
binary = sys.argv[1]
WINE = len(sys.argv) > 2 and sys.argv[2] == "wine"
here = os.path.dirname(os.path.abspath(__file__))
td = tempfile.mkdtemp(prefix="clipboard-check-")

def free_display():
    for n in range(300, 400):
        if not os.path.exists("/tmp/.X11-unix/X%d" % n) and not os.path.exists("/tmp/.X%d-lock" % n):
            return n
n = free_display()
xvfb = subprocess.Popen(["Xvfb", ":%d" % n, "-screen", "0", "800x600x24", "-nolisten", "tcp"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
env = dict(os.environ, DISPLAY=":%d" % n, FIRN_CLIPBOARD="native")
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

def run_prog(*args, out, timeout=90):
    p = prog(*args, out=out)
    p.wait(timeout=timeout)
    return open(out, "rb").read()

def wait_for(path, text, timeout=60):
    end = time.time() + timeout
    while time.time() < end:
        if os.path.exists(path) and text.encode() in open(path, "rb").read():
            return True
        time.sleep(0.2)
    return False

def host(path):
    return ("Z:" + path.replace("/", "\\")) if WINE else path

def gtk(*args, timeout=60):
    return subprocess.run([sys.executable, os.path.join(here, "gtk_peer.py")] + list(args), capture_output=True, text=True, env=env, timeout=timeout)

def got_payload(data):
    # "GOT <n>\n<n octets>\n"
    if not data.startswith(b"GOT "):
        return None
    head, rest = data.split(b"\n", 1)
    return rest[:int(head[4:])]

def sha(b):
    return hashlib.sha256(b).hexdigest()

def wine_kill():
    if WINE:
        subprocess.run(["wineserver", "-k"], env=env)

def stop(p):
    p.terminate()
    try:
        p.wait(timeout=10)
    except subprocess.TimeoutExpired:
        p.kill()
    wine_kill()

TEXT = "héllo wörld € 日本語 \U0001f600\nline two\n\nlast line"   # english: ok (UTF-8 test data)
BIG = "".join("row %06d äöü 0123456789abcdef0123456789abcdef\n" % i for i in range(26000))   # about 1.5 MiB; english: ok (UTF-8 test data)
pngf = os.path.join(td, "p.png")
subprocess.run([sys.executable, "-I", os.path.join(here, "png_tool.py"), "make", pngf], check=True)
want_png = subprocess.run([sys.executable, "-I", os.path.join(here, "png_tool.py"), "digest", pngf], capture_output=True, text=True).stdout.strip()
try:
    print("   backend: " + run_prog("name", out=os.path.join(td, "name")).decode().strip())
    # --- 1. the program owns the clipboard, GTK reads it
    for label, text in (("text", TEXT), ("1.5 MiB text", BIG)):
        tf = os.path.join(td, "t.txt")
        open(tf, "w", encoding="utf-8", newline="").write(text)
        o = os.path.join(td, "o1")
        p = prog("copy", host(tf), out=o)
        check("the program copied the %s" % label, wait_for(o, "READY"), open(o, "rb").read() if os.path.exists(o) else b"")
        check("copy answered true (%s)" % label, b"COPY ok" in open(o, "rb").read())
        time.sleep(1.0 if not WINE else 2.0)
        outf = os.path.join(td, "gtk_text")
        g = gtk("get-text", outf)
        got = open(outf, "r", encoding="utf-8", newline="").read() if os.path.exists(outf) and g.stdout.startswith("TEXT") else None
        want = text.replace("\n", "\n")
        check("GTK reads the program's %s exactly" % label, got == want, (g.stdout + g.stderr)[:200] + " | got %s octets" % (len(got.encode()) if got is not None else None))
        if os.path.exists(outf):
            os.unlink(outf)
        stop(p)
        time.sleep(1.5)
    o = os.path.join(td, "o2")
    p = prog("copypng", host(pngf), out=o)
    check("the program copied the PNG", wait_for(o, "READY"), open(o, "rb").read() if os.path.exists(o) else b"")
    time.sleep(1.0 if not WINE else 2.0)
    g = gtk("get-png")
    check("GTK decodes the program's PNG to the same pixels", g.stdout.strip() == want_png, (g.stdout + g.stderr)[:300])
    stop(p)
    time.sleep(1.5)
    # --- 2. GTK owns the clipboard, the program reads it
    for label, text in (("text", TEXT), ("1.5 MiB text", BIG)):
        tf = os.path.join(td, "g.txt")
        open(tf, "w", encoding="utf-8", newline="").write(text)
        gp = subprocess.Popen([sys.executable, os.path.join(here, "gtk_peer.py"), "set-text", tf], stdout=subprocess.PIPE, text=True, env=env)
        procs.append(gp)
        check("GTK owns the %s" % label, gp.stdout.readline().strip() == "READY")
        time.sleep(0.5)
        data = run_prog("paste", out=os.path.join(td, "o3"))
        pl = got_payload(data)
        check("the program pastes GTK's %s exactly" % label, pl is not None and pl.decode("utf-8", "replace") == text, data[:120])
        h = run_prog("has", out=os.path.join(td, "o4")).decode()
        check("has_text is true while GTK's text is there (%s)" % label, "text=1" in h, h)
        gp.terminate()
        gp.wait(timeout=10)
        time.sleep(1.0)
    gp = subprocess.Popen([sys.executable, os.path.join(here, "gtk_peer.py"), "set-png", pngf], stdout=subprocess.PIPE, text=True, env=env)
    procs.append(gp)
    check("GTK owns the picture", gp.stdout.readline().strip() == "READY")
    time.sleep(0.5)
    data = run_prog("pastepng", out=os.path.join(td, "o5"))
    pl = got_payload(data)
    check("the program pastes a PNG from GTK", pl is not None and pl[:8] == b"\x89PNG\r\n\x1a\n", data[:80])
    if pl is not None:
        gf = os.path.join(td, "from_gtk.png")
        open(gf, "wb").write(pl)
        d = subprocess.run([sys.executable, "-I", os.path.join(here, "png_tool.py"), "digest", gf], capture_output=True, text=True).stdout.strip()
        check("and its pixels are the picture GTK was given", d == want_png, d)
    h = run_prog("has", out=os.path.join(td, "o6")).decode()
    check("has_png is true while GTK's picture is there", "png=1" in h, h)
    gp.terminate()
    gp.wait(timeout=10)
    time.sleep(1.0)
    # --- 3. clear takes it away from a foreign owner
    tf = os.path.join(td, "c.txt")
    open(tf, "w", encoding="utf-8").write("to be cleared")
    gp = subprocess.Popen([sys.executable, os.path.join(here, "gtk_peer.py"), "set-text", tf], stdout=subprocess.PIPE, text=True, env=env)
    procs.append(gp)
    check("GTK owns a text again", gp.stdout.readline().strip() == "READY")
    time.sleep(0.5)
    c = run_prog("clear", out=os.path.join(td, "o7")).decode()
    check("clear answered true", "CLEAR ok" in c, c)
    time.sleep(0.5)
    g = gtk("get-text", os.path.join(td, "after"), timeout=60)
    check("GTK finds no text after the program cleared the clipboard", g.stdout.startswith("NOTEXT"), g.stdout + g.stderr)
    h = run_prog("has", out=os.path.join(td, "o8")).decode()
    check("has_text / has_png are false after clear", "text=0" in h and "png=0" in h, h)
    gp.terminate()
finally:
    for p in procs[::-1]:
        try:
            p.terminate()
        except Exception:
            pass
    if WINE:
        subprocess.run(["wineserver", "-k"], env=env)
    shutil.rmtree(td, ignore_errors=True)
print("clipboard: %d failed" % len(FAILED) if FAILED else "clipboard: all checks passed")
sys.exit(1 if FAILED else 0)
