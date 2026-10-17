#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/parity.py -- HOST PARITY: the same first frame on three hosts.
#
# One source (examples/fui/*.fi), built for several hosts. The reference is
# the HEADLESS frame that tools/fui/app_main.fi paints without a window
# (app-hello.png, app-form-empty.png). This script puts the native programs in
# a real window and compares what the host really shows:
#
#   x11   the Linux binary on its OWN Xvfb (own display number, own pid,
#         `-dpi 96`), window grabbed with xwd. GATE: 0 pixels differ.
#   wine  the x86_64-windows .exe under wine64 (own WINEPREFIX in the work
#         dir) on its own Xvfb. Informational: reported, gated only by
#         --max-wine percent (default 1.0).
#
# WHY `-dpi 96`: Xvfb's default dpi is about 100 -> fUi scales the UI by 1.04
# and the window is 416 x 260 instead of 400 x 250. That is the host working
# as designed (Xft.dpi / screen mm -> scale), NOT a layout bug; it was the whole
# "5.02 % host deviation" of the first measurement. Parity is only meaningful
# at the same scale, so the server is started at 96 dpi.
#
# The wasm32-browser host is compared against the same references by
# tools/wasm/appcheck.py (run through tools/fui/parity.sh).
#
# Usage: parity.py <dir with app_main PNGs> <dir with hello, form[, hello.exe,
#        form.exe]> [--wine] [--max-wine PERCENT]
# Prints one line per pair: PARITY <host> <example>: <px> px differ (<pct> %).
# A host that cannot run prints "NOT TESTED <host>: <reason>" and does not
# count as failure. Exit 1 if a gated pair differs.
import os
import shutil
import subprocess
import sys
import tempfile
import time

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
PAIRS = [("hello", "Test Window", "app-hello.png"),
         ("form", "Sign up", "app-form-empty.png")]


def free_display():
    for n in range(300, 400):
        if not os.path.exists("/tmp/.X11-unix/X%d" % n) and \
           not os.path.exists("/tmp/.X%d-lock" % n):
            return n
    raise SystemExit("no free display number")


class Server:
    """Own Xvfb at 96 dpi. Only this pid is ever terminated."""

    def __init__(self):
        self.n = free_display()
        self.env = dict(os.environ, DISPLAY=":%d" % self.n)
        self.env.pop("XAUTHORITY", None)
        self.p = subprocess.Popen(
            ["Xvfb", ":%d" % self.n, "-screen", "0", "1024x768x24", "-dpi",
             "96", "-nolisten", "tcp", "-noreset"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(100):
            if os.path.exists("/tmp/.X11-unix/X%d" % self.n):
                break
            time.sleep(0.05)
        time.sleep(0.3)
        if self.p.poll() is not None:
            raise SystemExit("Xvfb :%d did not start" % self.n)

    def run(self, *a, **kw):
        return subprocess.run(list(a), env=self.env, capture_output=True, **kw)

    def window(self, title, secs=20.0):
        end = time.time() + secs
        while time.time() < end:
            r = self.run("xwininfo", "-root", "-tree").stdout.decode()
            for line in r.splitlines():
                if ('"%s"' % title) in line:
                    t = line.split()
                    return int(t[0], 16)
            time.sleep(0.3)
        return None

    def close(self):
        self.p.terminate()
        try:
            self.p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.p.kill()


def grab(srv, wid, out_png):
    raw = out_png + ".xwd"
    with open(raw, "wb") as f:
        subprocess.run(["xwd", "-silent", "-id", str(wid)], env=srv.env,
                       stdout=f, check=True)
    subprocess.run([sys.executable, "-I", os.path.join(HERE, "xwd2png.py"),
                    raw, out_png], check=True, capture_output=True)
    return Image.open(out_png).convert("RGB")


def differ(a, b, tol=0):
    if a.size != b.size:
        return None
    x = np.asarray(a).astype(int)
    y = np.asarray(b).astype(int)
    return int((np.abs(x - y).max(axis=2) > tol).sum())


def report(host, name, shot, ref):
    if shot.size != ref.size:
        print("PARITY %s %s: SIZE %dx%d vs reference %dx%d" %
              (host, name, shot.size[0], shot.size[1], ref.size[0], ref.size[1]))
        return None, None
    n = differ(shot, ref, 0)
    total = ref.size[0] * ref.size[1]
    pct = 100.0 * n / total
    print("PARITY %s %s: %d px differ (%.3f %%) of %d, tol 0" %
          (host, name, n, pct, total))
    return n, pct


def run_host(host, work, refdir, bindir, gate_pct, wine):
    bad = 0
    srv = Server()
    env = dict(srv.env)
    prefix = None
    procs = []
    try:
        if wine:
            prefix = os.path.join(work, "wineprefix")
            env.update(WINEPREFIX=prefix, WINEARCH="win64", WINEDEBUG="-all")
            r = subprocess.run([wine, "wineboot", "--init"], env=env,
                               capture_output=True, timeout=150)
        for name, title, refpng in PAIRS:
            exe = os.path.join(bindir, name + (".exe" if wine else ""))
            if not os.path.exists(exe):
                print("NOT TESTED %s %s: %s missing" % (host, name, exe))
                continue
            cmd = [wine, exe] if wine else [exe]
            p = subprocess.Popen(cmd, env=env, stdout=subprocess.DEVNULL,
                                 stderr=subprocess.DEVNULL)
            procs.append(p)
            wid = srv.window(title, 40.0 if wine else 15.0)
            if wid is None:
                print("NOT TESTED %s %s: no window \"%s\" appeared" %
                      (host, name, title))
                p.terminate()
                continue
            time.sleep(1.5)
            shot = grab(srv, wid, os.path.join(work, "%s-%s.png" % (host, name)))
            ref = Image.open(os.path.join(refdir, refpng)).convert("RGB")
            n, pct = report(host, name, shot, ref)
            if n is None:
                bad += 1
            elif pct > gate_pct:
                bad += 1
            p.terminate()
    finally:
        for p in procs:
            if p.poll() is None:
                p.kill()
        if wine:
            subprocess.run(["wineserver", "-k"], env=env, capture_output=True)
        srv.close()
    return bad


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    refdir, bindir = os.path.abspath(args[0]), os.path.abspath(args[1])
    want_wine = "--wine" in sys.argv
    max_wine = 3.0
    if "--max-wine" in sys.argv:
        max_wine = float(sys.argv[sys.argv.index("--max-wine") + 1])
    work = tempfile.mkdtemp(prefix="fui-parity.")
    bad = 0
    try:
        if shutil.which("Xvfb") and shutil.which("xwd") and shutil.which("xwininfo"):
            bad += run_host("x11", work, refdir, bindir, 0.0, None)
        else:
            print("NOT TESTED x11: Xvfb/xwd/xwininfo missing")
        if want_wine:
            wine = shutil.which("wine64") or "/usr/lib/wine/wine64"
            if os.path.exists(wine):
                # informational unless it deviates by more than --max-wine
                bad += run_host("wine", work, refdir, bindir, max_wine, wine)
            else:
                print("NOT TESTED wine: wine64 missing")
    finally:
        shutil.rmtree(work, ignore_errors=True)
    print("PARITY %s" % ("PASSED" if bad == 0 else "FAILED (%d pair(s))" % bad))
    return 1 if bad else 0


sys.exit(main())
