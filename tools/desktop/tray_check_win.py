#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/tray_check_win.py -- desktop.tray and desktop.notify, Windows build, under Wine.
#
#   usage: tray_check_win.py <tray_main.exe> <wintray_poke.exe> <notify_main.exe> <repo root>
#
# Wine's `explorer /desktop=...` plays the Windows shell: it owns Shell_TrayWnd, so Shell_NotifyIconW works
# for real (without it the call fails, which the first check shows). What is checked:
#   * Shell_NotifyIconW(NIM_ADD / NIF_INFO) is accepted;
#   * the icon is DRAWN in the taskbar tray: the screenshot of the X server shows the colours of the
#     program's picture (a 4 x 2 RGBA image: red, green, blue, white; the columns in that order);
#   * the messages a shell and a popup menu send (tools/desktop/wintray_poke.exe posts them to the
#     program's window) become the same events as on Linux: click, middle click, context menu, the menu
#     item chosen, the balloon clicked / timed out.
import os, shutil, subprocess, sys, tempfile, time

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

for tool in ("wine", "Xvfb", "xwd"):
    if shutil.which(tool) is None:
        print("  SKIP  %s not installed" % tool)
        sys.exit(0)
tray_exe, poke_exe, notify_exe, root = [os.path.abspath(a) for a in sys.argv[1:5]]
td = tempfile.mkdtemp(prefix="tray-win-")

def free_display():
    for n in range(300, 350):
        if not os.path.exists("/tmp/.X11-unix/X%d" % n) and not os.path.exists("/tmp/.X%d-lock" % n):
            return n
n = free_display()
xvfb = subprocess.Popen(["Xvfb", ":%d" % n, "-screen", "0", "800x600x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
env = dict(os.environ, DISPLAY=":%d" % n, WINEPREFIX=os.environ.get("WINEPREFIX", os.path.expanduser("~/.wine-firn")), WINEDEBUG="-all")
for _ in range(100):
    if os.path.exists("/tmp/.X11-unix/X%d" % n):
        break
    time.sleep(0.1)
time.sleep(0.3)
procs = [xvfb]
DESK = ["wine", "explorer", "/desktop=shell,800x600"]

def start(exe, *args, out):
    f = open(out, "wb")
    p = subprocess.Popen(DESK + [exe] + list(args), stdout=f, stderr=subprocess.DEVNULL, env=env)
    procs.append(p)
    return p

def wait_for(path, text, timeout=60):
    end = time.time() + timeout
    while time.time() < end:
        if os.path.exists(path) and text.encode() in open(path, "rb").read():
            return True
        time.sleep(0.2)
    return False

def poke(*args):
    r = subprocess.run(DESK + [poke_exe] + list(args), capture_output=True, text=True, env=env, timeout=60)
    time.sleep(1.2)
    return r

def lines(path):
    return open(path, "rb").read().decode("utf-8", "replace").splitlines()

try:
    # ---- a program without any shell: the shell refuses the icon
    subprocess.run(["wineserver", "-k"], env=env)       # no explorer left over from an earlier check
    time.sleep(1.5)
    r = subprocess.run(["wine", tray_exe], capture_output=True, text=True, env=env, timeout=60)
    # Wine may or may not have started its own shell by now (it does when it boots a fresh prefix session):
    # either way the program must say what the shell did, and `VISIBLE` must agree with `SHOWN`.
    o0 = r.stdout.split()
    check("a program started without our shell learns whether the tray took the icon (SHOWN yes/no, VISIBLE the same)",
          o0[:2] in (["SHOWN", "yes"], ["SHOWN", "no"]) and o0[2:4] == ["VISIBLE", o0[1]], (r.stdout, r.stderr[-200:]))
    subprocess.run(["wineserver", "-k"], env=env)
    time.sleep(1)
    # ---- with the shell
    o1 = os.path.join(td, "tray.out")
    p1 = start(tray_exe, "balloon", out=o1)
    check("the shell accepts the icon (SHOWN yes)", wait_for(o1, "SHOWN yes"), lines(o1) if os.path.exists(o1) else None)
    check("the balloon is accepted (NIF_INFO)", wait_for(o1, "BALLOON yes"), lines(o1))
    time.sleep(2.0)
    shot = os.path.join(td, "shot.xwd")
    subprocess.run(["xwd", "-root", "-display", env["DISPLAY"], "-out", shot], check=True, env=env)
    png = os.path.join(td, "shot.png")
    subprocess.run([sys.executable, os.path.join(root, "tools/fui/xwd2png.py"), shot, png], capture_output=True)
    try:
        from PIL import Image
        im = Image.open(png).convert("RGB")
        px = im.load()
        W, H = im.size
        def find(c):
            return [(x, y) for y in range(H) for x in range(W) if px[x, y] == c]
        red, green, blue = find((255, 0, 0)), find((0, 255, 0)), find((0, 0, 255))
        check("the icon is drawn in the tray: red, green and blue of the picture are on the screen", len(red) > 0 and len(green) > 0 and len(blue) > 0, (len(red), len(green), len(blue)))
        if red and green and blue:
            check("... in the order of the picture's columns (red left of green left of blue), side by side",
                  min(x for x, y in red) < min(x for x, y in green) < min(x for x, y in blue)
                  and abs(min(y for x, y in red) - min(y for x, y in green)) <= 1, (red[:1], green[:1], blue[:1]))
            check("... and the 4x2 picture is scaled evenly (equal width per colour)",
                  len(set(len(c) for c in (red, green, blue))) == 1, (len(red), len(green), len(blue)))
    except ImportError:
        print("  SKIP  PIL missing: the icon pixels were not read")
    poke("click")
    poke("middle")
    poke("right")           # builds and shows the popup menu: the program is inside TrackPopupMenu
    poke("cmd", "3")        # the third visible item (Gray, id 4)
    poke("cancel")          # closes the menu
    poke("click")
    poke("cmd", "1")        # Open (id 1)
    poke("cmd", "4")        # Quit (id 2)
    p1.wait(timeout=60)
    ev = [tuple(int(x) for x in l.split()[1:]) for l in lines(o1) if l.startswith("EV ")]
    check("left click -> EV_ACTIVATE (1) at the cursor", any(e[0] == 1 for e in ev), ev)
    check("middle click -> EV_SECONDARY (2)", any(e[0] == 2 for e in ev), ev)
    check("right click -> EV_CONTEXT (4), then the menu", any(e[0] == 4 for e in ev), ev)
    check("a menu item chosen -> EV_MENU with the item's own id (3rd visible item = 4)", (3, 4, 0, 0) in ev, ev)
    check("menu item 1 -> id 1; the last one -> id 2 (Quit ends the program)", (3, 1, 0, 0) in ev and (3, 2, 0, 0) in ev, ev)
    check("the program ended by itself and says VISIBLE yes", any("VISIBLE yes" in l for l in lines(o1)), lines(o1))
    subprocess.run(["wineserver", "-k"], env=env)
    time.sleep(1)

    # ---- the notifier on top of the tray
    o2 = os.path.join(td, "notify.out")
    p2 = start(notify_exe, out=o2)
    check("the notifier gets the shell's tray (READY yes)", wait_for(o2, "READY yes"), lines(o2) if os.path.exists(o2) else None)
    check("notify GO", wait_for(o2, "GO"), lines(o2))
    L = lines(o2)
    check("server information and capabilities answered", any(l.startswith("INFO") for l in L) and "CAPS 2" in L, L)
    check("first id 1, replacing keeps it", "ID 1" in L and "ID2 1" in L, L)
    check("the balloon was closed by the program", "CLOSE ok" in L, L)
    poke("balloonclick")
    check("a click on the balloon -> EV_ACTION with the key `default`", wait_for(o2, "ACTION"), lines(o2))
    poke("balloontimeout")
    check("a balloon that timed out -> EV_CLOSED, reason 1 (expired)", wait_for(o2, "CLOSED 0 1"), lines(o2))
    p2.terminate()
finally:
    for p in procs[::-1]:
        try:
            p.terminate()
        except Exception:
            pass
    subprocess.run(["wineserver", "-k"], env=env)
    shutil.rmtree(td, ignore_errors=True)
print("tray (Windows): %d failed" % len(FAILED) if FAILED else "tray (Windows): all checks passed")
sys.exit(1 if FAILED else 0)
