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
# A PROG ending in .exe is a Windows build: it runs under wine on the same kind of Xvfb
# (r172: the Win32 popup window). Wine puts the window at a place of its own, so every
# coordinate and screenshot below is shifted by the window's origin (OX, OY).
# Without Xvfb / xdotool / xwd / PIL (/ wine): "SKIP", exit 0.
import os, shutil, subprocess, sys, tempfile, time

PROG = sys.argv[1]
PROOF = sys.argv[2]
os.makedirs(PROOF, exist_ok=True)
HERE = os.path.dirname(os.path.abspath(__file__))
WINE = PROG.endswith(".exe")
OX = 0
OY = 0
for tool in ("Xvfb", "xdotool", "xwd") + (("wine",) if WINE else ()):
    if shutil.which(tool) is None:
        print("SKIP: no %s -- no real window in this run" % tool)
        sys.exit(0)
try:
    from PIL import Image
except Exception:
    print("SKIP: no PIL (python3-pil) -- the pictures cannot be read")
    sys.exit(0)

sys.path.insert(0, HERE)
import xlive

bad = 0
total = 0


def chk(text, ok, got="", want=""):
    global bad, total
    total += 1
    if not ok:
        bad += 1
    print("  %-78s%s %s %s" % (text, "OK   " if ok else "WRONG", got, want), flush=True)


xvfb, DISP = xlive.start_xvfb()
env = dict(os.environ, DISPLAY=DISP)
app = None


def finish(code):
    if app is not None:
        app.terminate()
        try:
            app.wait(3)
        except Exception:
            app.kill()
    if WINE:
        subprocess.run(["wineserver", "-k"], env=env, check=False)
        shutil.rmtree(env.get("WINEPREFIX", "/nonexistent-ctxlive"), ignore_errors=True)
    xvfb.terminate()
    try:
        xvfb.wait(3)
    except Exception:
        xvfb.kill()
    sys.exit(code)


def xdo(*args):
    a = list(args)
    if a and a[0] == "mousemove" and len(a) >= 3:
        a[1] = str(int(a[1]) + OX)
        a[2] = str(int(a[2]) + OY)
    subprocess.run(["xdotool"] + a, env=env, check=False)


def grab(name="ctx-live-probe"):
    img = xlive.capture(DISP, PROOF, name)
    if OX or OY:
        shifted = Image.new("RGB", img.size, (0, 0, 0))
        shifted.paste(img.crop((OX, OY, img.size[0], img.size[1])), (0, 0))
        img = shifted
    return img


def await_state(name, pred, action=None, timeout=20.0, retry_after=5.0):
    img, ok = xlive.wait_until(grab, pred, action, timeout, retry_after)
    img.save(os.path.join(PROOF, name + ".png"))
    return img


def cpu_ticks(pid):
    with open("/proc/%d/stat" % pid) as f:
        parts = f.read().rsplit(")", 1)[1].split()
    return int(parts[11]) + int(parts[12])


def diff(a, b, box):
    return xlive.count_diff(a, b, box)


def escape():
    xdo("key", "Escape")


def click_right_at(x, y):
    xdo("mousemove", str(x), str(y))
    time.sleep(0.1)
    xdo("click", "3")


def open_menu(ref, name, probe_box=(445, 150, 590, 395), attempts=4):
    img = None
    for _ in range(attempts):
        click_right_at(300, 186)
        img = await_state(name, lambda i: diff(ref, i, probe_box) > 1500, timeout=8.0)
        if diff(ref, img, probe_box) > 1500:
            return img
        escape()
        await_state(name, lambda i: diff(ref, i, probe_box) < 50, escape, timeout=6.0, retry_after=2.0)
    return img


def settle(timeout=10.0):
    prev = grab()
    deadline = time.time() + timeout
    while time.time() < deadline:
        time.sleep(0.3)
        cur = grab()
        if diff(prev, cur, (0, 0, xlive.SCREEN_W, xlive.SCREEN_H)) == 0:
            return cur
        prev = cur
    return prev


print("== a context menu in a real window (Xvfb %s) ==" % DISP)
cmd = [PROG]
if WINE:
    wpfx = tempfile.mkdtemp(prefix="ctxlive-wine-")
    env = dict(env, WINEPREFIX=wpfx, WINEDEBUG="-all")
    subprocess.run(["wineboot", "-i"], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=300)
    cmd = ["wine", PROG]
app = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
if WINE:
    deadline = time.time() + 90
    while time.time() < deadline:
        time.sleep(0.5)
        out = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "Context"], env=env,
                             capture_output=True, text=True).stdout.split()
        if out:
            g = subprocess.run(["xdotool", "getwindowgeometry", out[0]], env=env, capture_output=True,
                               text=True).stdout
            if "Position:" not in g:
                continue
            pos = g.split("Position:")[1].split()[0].split(",")
            OX, OY = int(pos[0]), int(pos[1])
            subprocess.run(["xdotool", "windowfocus", out[0]], env=env, check=False)
            break
    print("  (wine window origin %d,%d)" % (OX, OY))
black = Image.new("RGB", (xlive.SCREEN_W, xlive.SCREEN_H), (0, 0, 0))
A = await_state("ctx-live-0-closed", lambda i: diff(i, black, (0, 0, 420, 360)) > 5000, timeout=45.0)
if app.poll() is not None:
    chk("the program is running", False, "exit %s" % app.returncode, app.stderr.read().decode()[:200])
    finish(1)
A = settle()
A.save(os.path.join(PROOF, "ctx-live-0-closed.png"))
chk("the program's window is up (the root is not all black)", diff(A, black, (0, 0, 420, 360)) > 5000)

B = open_menu(A, "ctx-live-1-open")
n_out = diff(A, B, (445, 150, 590, 395))
chk("the menu is visible OUTSIDE the window (right of x = 420)", n_out > 1500, "%d px" % n_out)
n_in = diff(A, B, (300, 186, 430, 336))
chk("... and inside the window too", n_in > 1500, "%d px" % n_in)
rows_ok = 0
pb = B.load()
for y in range(196, 330):
    if sum(abs(p - q) for p, q in zip(pb[430, y], pb[444, y])) < 40:
        rows_ok += 1
chk("the menu is not cut at the window's edge (x = 430 and x = 444 look alike)", rows_ok > 100, "%d rows" % rows_ok)

escape()
C = await_state("ctx-live-2-closed", lambda i: diff(A, i, (440, 0, 1100, 720)) < 50, escape, retry_after=4.0)
n = diff(A, C, (440, 0, 1100, 720))
chk("Esc closes the menu: the screen outside the window is as before", n < 50, "%d px differ" % n)

B = open_menu(A, "ctx-live-1b-open")
hover_box = (305, 222, 470, 256)
nudge = lambda: (xdo("mousemove", "349", "239"), time.sleep(0.1), xdo("mousemove", "350", "239"))
xdo("mousemove", "350", "239")
D = await_state("ctx-live-3-hover", lambda i: diff(B, i, hover_box) > 300, nudge, retry_after=3.0)
n = diff(B, D, hover_box)
chk("hovering Rename highlights it", n > 300, "%d px" % n)
xdo("click", "1")
E = await_state("ctx-live-4-ran",
                lambda i: diff(A, i, (445, 150, 590, 395)) < 50 and diff(A, i, (130, 255, 290, 285)) > 100)
n = diff(A, E, (445, 150, 590, 395))
chk("the click ran the action: the menu is gone", n < 50, "%d px" % n)
n = diff(A, E, (130, 255, 290, 285))
chk("... and the page's label changed ('rename')", n > 100, "%d px" % n)

E = settle()
B = open_menu(E, "ctx-live-5a-open")
xdo("mousemove", "800", "500")
time.sleep(0.2)
xdo("click", "1")
if WINE:
    F = await_state("ctx-live-5-outside", lambda i: False, timeout=1.0)
    print("  NOT TESTED wine: a click on the empty screen (no window manager, no activation change)")
    escape()
    await_state("ctx-live-5b-closed", lambda i: diff(E, i, (445, 150, 590, 395)) < 50, escape, retry_after=3.0)
else:
    F = await_state("ctx-live-5-outside", lambda i: diff(E, i, (445, 150, 590, 395)) < 50)
    n = diff(E, F, (445, 150, 590, 395))
    chk("a click on the empty screen closes the menu", n < 50, "%d px" % n)

F = settle()
G0 = open_menu(F, "ctx-live-6-open")
xdo("key", "End")
xdo("key", "Up")
time.sleep(0.2)
xdo("key", "Right")
G1 = await_state("ctx-live-7-submenu",
                 lambda i: diff(G0, i, (300, 186, 800, 450)) > 800 and diff(G0, i, (612, 140, 800, 450)) > 600)
n = diff(G0, G1, (300, 186, 800, 450))
chk("the keys reach the menu (the focus moved, the submenu opened)", n > 800, "%d px" % n)
n = diff(G0, G1, (612, 140, 800, 450))
chk("the submenu is a window of its own to the right of the menu", n > 600, "%d px" % n)
escape()
G2 = await_state("ctx-live-8-subclosed",
                 lambda i: diff(G0, i, (612, 140, 800, 450)) < 50 and diff(G1, i, (300, 186, 570, 424)) < 400,
                 escape, retry_after=4.0)
n = diff(G1, G2, (300, 186, 570, 424))
chk("Esc closes the submenu only: the menu stays as it was", n < 400, "%d px differ" % n)
n = diff(G0, G2, (612, 140, 800, 450))
chk("... and the submenu's window is gone", n < 50, "%d px differ" % n)
escape()
G3 = await_state("ctx-live-9-closed",
                 lambda i: diff(F, i, (440, 0, 1100, 720)) < 50 and diff(i, G0, (440, 186, 800, 450)) > 500,
                 escape, retry_after=4.0)
n = diff(F, G3, (440, 0, 1100, 720))
chk("Esc again closes the menu", diff(G3, G0, (440, 186, 800, 450)) > 500 and n < 50, "%d px" % n)

G3 = settle()
xdo("key", "Tab")
time.sleep(0.2)
xdo("key", "shift+F10")
H = await_state("ctx-live-10-keyboard", lambda i: diff(G3, i, (90, 250, 560, 560)) > 1500)
n = diff(G3, H, (90, 250, 560, 560))
chk("Tab, Shift+F10: a menu opens for the focused element", n > 1500, "%d px" % n)
escape()
I = await_state("ctx-live-11-closed", lambda i: diff(H, i, (90, 250, 560, 560)) > 1500, escape, retry_after=4.0)
chk("Esc closes it", diff(H, I, (90, 250, 560, 560)) > 1500)

I = settle()
open_menu(I, "ctx-live-12-open")
settle()
t0 = cpu_ticks(app.pid)
time.sleep(2.0)
t1 = cpu_ticks(app.pid)
chk("an open menu does not spin: under 10 clock ticks of CPU in 2 s", t1 - t0 < 10, "%d ticks" % (t1 - t0))
escape()
time.sleep(0.4)

chk("the program is still running", app.poll() is None)

print("")
print("CTXLIVE %s (%d of %d)" % ("PASSED" if bad == 0 else "FAILED", total - bad, total))
finish(0 if bad == 0 else 1)
