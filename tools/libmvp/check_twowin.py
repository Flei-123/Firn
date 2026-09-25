#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/libmvp/check_twowin.py <twowin_probe> -- two top-level windows of
# one program (OpenPlan LIB-005) on an Xvfb of its own: both exist with
# their titles and colours, closing one leaves the other running, closing
# the other ends the program.
import os, shutil, subprocess, sys, time
probe = sys.argv[1]
if not shutil.which("Xvfb"):
    print("  skip: no Xvfb"); sys.exit(0)
try:
    from Xlib import display, X, protocol
except ImportError:
    print("  skip: no python-xlib"); sys.exit(0)
disp = None
for n in range(160, 180):
    if not os.path.exists("/tmp/.X11-unix/X%d" % n):
        disp = n; break
xvfb = subprocess.Popen(["Xvfb", ":%d" % disp, "-screen", "0", "640x480x24", "-nolisten", "tcp"],
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
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
try:
    env = dict(os.environ, DISPLAY=":%d" % disp)
    p = subprocess.Popen([probe], env=env, stdout=subprocess.PIPE, text=True)
    first = p.stdout.readline().strip()
    (ok if first == "READY" else fail)("two windows opened (%s)" % first)
    d = display.Display(":%d" % disp)
    root = d.screen().root
    time.sleep(0.5)
    wins = {}
    for w in root.query_tree().children:
        name = w.get_wm_name()
        if name in ("Firn A", "Firn B"):
            wins[name] = w
    (ok if sorted(wins) == ["Firn A", "Firn B"] else fail)("both are top-level windows with their titles (%s)" % sorted(wins))
    def colour(w):
        # the bottom right corner: without a window manager both windows
        # sit at 0,0, and B (160 x 100) covers the top left of A (200 x 120)
        g = w.get_geometry()
        img = w.get_image(g.width - 5, g.height - 5, 1, 1, X.ZPixmap, 0xFFFFFFFF)
        b, gr, r = img.data[0], img.data[1], img.data[2]
        return (r, gr, b)
    if len(wins) == 2:
        ca, cb = colour(wins["Firn A"]), colour(wins["Firn B"])
        (ok if ca == (255, 0, 0) and cb == (0, 0, 255) else fail)("each window shows its own picture (A %s, B %s)" % (ca, cb))
        wm_protocols = d.intern_atom("WM_PROTOCOLS")
        wm_delete = d.intern_atom("WM_DELETE_WINDOW")
        def close(w):
            ev = protocol.event.ClientMessage(window=w, client_type=wm_protocols, data=(32, [wm_delete, X.CurrentTime, 0, 0, 0]))
            w.send_event(ev, event_mask=0)
            d.flush()
        t0 = time.time()
        close(wins["Firn B"])
        line = p.stdout.readline().strip()
        dt = time.time() - t0
        (ok if line == "CLOSED 1" and dt < 1.0 else fail)("closing B is seen at once (%s after %.0f ms), A keeps running" % (line, dt * 1000))
        time.sleep(0.3)
        alive = p.poll() is None
        (ok if alive and colour(wins["Firn A"]) == (255, 0, 0) else fail)("A is still there and red")
        close(wins["Firn A"])
        rest = p.communicate(timeout=10)[0].split()
        (ok if rest == ["CLOSED", "0", "DONE"] and p.returncode == 0 else fail)("closing A ends the program (%s, exit %s)" % (rest, p.returncode))
    else:
        p.kill()
finally:
    xvfb.terminate()
print("twowin: %d failed" % bad)
sys.exit(1 if bad else 0)
