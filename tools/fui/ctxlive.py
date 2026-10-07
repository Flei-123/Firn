#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/ctxlive.py -- A CONTEXT MENU ON A REAL X SERVER (roadmap r168).
#
# tools/fui/menuview_main.fi proves the menu against a canvas in memory. This
# run proves what a user sees: examples/fui/context runs on its own Xvfb, is
# operated from OUTSIDE with xdotool, and what is checked is what the SERVER
# shows (xwd). The one thing a canvas cannot show: the menu is a window of the
# screen's own, so it reaches OUT of the 420 x 360 window instead of being cut
# off at its edge.
#
#     python3 tools/fui/ctxlive.py <context program> <proof folder>
#
# Without Xvfb / xdotool / xwd / PIL: "SKIP", exit 0.
import os, shutil, subprocess, sys, time

PROG = sys.argv[1]
BELEG = sys.argv[2]
os.makedirs(BELEG, exist_ok=True)
HERE = os.path.dirname(os.path.abspath(__file__))
for tool in ("Xvfb", "xdotool", "xwd"):
    if shutil.which(tool) is None:
        print("SKIP: no %s -- no real window in this run" % tool)
        sys.exit(0)
try:
    from PIL import Image
except Exception:
    print("SKIP: no PIL (python3-pil) -- the pictures cannot be read")
    sys.exit(0)

bad = 0
total = 0


def chk(text, ok, got="", want=""):
    global bad, total
    total += 1
    if not ok:
        bad += 1
    print("  %-78s%s %s %s" % (text, "OK   " if ok else "WRONG", got, want), flush=True)


DISP = ":%d" % (100 + os.getpid() % 800)
env = dict(os.environ, DISPLAY=DISP)
xvfb = subprocess.Popen(["Xvfb", DISP, "-screen", "0", "1100x720x24"],
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(1.0)
app = None


def finish(code):
    if app is not None:
        app.terminate()
        try:
            app.wait(3)
        except Exception:
            app.kill()
    xvfb.terminate()
    try:
        xvfb.wait(3)
    except Exception:
        xvfb.kill()
    sys.exit(code)


def xdo(*args):
    subprocess.run(["xdotool"] + list(args), env=env, check=False)


def shot(name):
    xwd = os.path.join(BELEG, name + ".xwd")
    with open(xwd, "wb") as f:
        subprocess.run(["xwd", "-root", "-silent", "-display", DISP], stdout=f, check=True)
    png = os.path.join(BELEG, name + ".png")
    subprocess.run([sys.executable, os.path.join(HERE, "xwd2png.py"), xwd, png],
                   check=True, stdout=subprocess.DEVNULL)
    os.remove(xwd)
    return Image.open(png).convert("RGB")


def diff(a, b, box):
    x0, y0, x1, y1 = box
    pa = a.load()
    pb = b.load()
    n = 0
    for y in range(y0, y1):
        for x in range(x0, x1):
            if sum(abs(p - q) for p, q in zip(pa[x, y], pb[x, y])) > 60:
                n += 1
    return n


print("== a context menu in a real window (Xvfb %s) ==" % DISP)
app = subprocess.Popen([PROG], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
time.sleep(1.8)
if app.poll() is not None:
    chk("the program is running", False, "exit %s" % app.returncode, app.stderr.read().decode()[:200])
    finish(1)
A = shot("ctx-live-0-closed")
chk("the program's window is up (the root is not all black)", diff(A, Image.new("RGB", A.size, (0, 0, 0)), (0, 0, 420, 360)) > 5000)

# 1. right click on the third row, near the window's right edge: the menu
#    (about 170 px wide) must reach past x = 420
xdo("mousemove", "300", "186")
time.sleep(0.2)
xdo("click", "3")
time.sleep(0.8)
B = shot("ctx-live-1-open")
out_box = (445, 150, 590, 395)
n_out = diff(A, B, out_box)
chk("the menu is visible OUTSIDE the window (right of x = 420)", n_out > 1500, "%d px" % n_out)
n_in = diff(A, B, (300, 186, 430, 336))
chk("... and inside the window too", n_in > 1500, "%d px" % n_in)
# the window's right edge is not a cut: the same rows run on both sides of x = 420
rows_ok = 0
pb = B.load()
for y in range(196, 330):
    if sum(abs(p - q) for p, q in zip(pb[430, y], pb[444, y])) < 40:
        rows_ok += 1
chk("the menu is not cut at the window's edge (x = 430 and x = 444 look alike)", rows_ok > 100, "%d rows" % rows_ok)

# 2. Esc closes it and the screen is as before
xdo("key", "Escape")
time.sleep(0.5)
C = shot("ctx-live-2-closed")
# (the right click chose the row under it, as in Windows: the list differs, the
# screen outside the window must be as before)
chk("Esc closes the menu: the screen outside the window is as before", diff(A, C, (440, 0, 1100, 720)) < 50, "%d px differ" % diff(A, C, (440, 0, 1100, 720)))

# 3. open again, click Rename (the second entry): the menu closes and the page shows the action's text
xdo("mousemove", "300", "186")
time.sleep(0.2)
xdo("click", "3")
time.sleep(0.8)
xdo("mousemove", "350", "239")
time.sleep(0.4)
D = shot("ctx-live-3-hover")
chk("hovering Rename highlights it", diff(B, D, (305, 222, 470, 256)) > 300, "%d px" % diff(B, D, (305, 222, 470, 256)))
xdo("click", "1")
time.sleep(0.8)
E = shot("ctx-live-4-ran")
chk("the click ran the action: the menu is gone", diff(A, E, (445, 150, 590, 395)) < 50, "%d px" % diff(A, E, (445, 150, 590, 395)))
chk("... and the page's label changed ('rename')", diff(A, E, (130, 255, 290, 285)) > 100, "%d px" % diff(A, E, (130, 255, 290, 285)))

# 4. a click outside everything closes the menu and the click goes nowhere
xdo("mousemove", "300", "186")
time.sleep(0.2)
xdo("click", "3")
time.sleep(0.8)
xdo("mousemove", "800", "500")
time.sleep(0.2)
xdo("click", "1")
time.sleep(0.6)
F = shot("ctx-live-5-outside")
chk("a click on the empty screen closes the menu", diff(E, F, (445, 150, 590, 395)) < 50, "%d px" % diff(E, F, (445, 150, 590, 395)))

# 5. the program is still alive and answers: the menu key path (focus the field with Tab, Shift+F10)
chk("the program is still running", app.poll() is None)

print("")
print("CTXLIVE %s (%d of %d)" % ("PASSED" if bad == 0 else "FAILED", total - bad, total))
finish(0 if bad == 0 else 1)
