#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/desktop/winnotify_check_win.py -- window.notify / window.stay_alive on Win32, under Wine.
#   usage: winnotify_check_win.py <winnotify_win_main.exe>
# Wine's `explorer /desktop=...` plays the shell (Shell_TrayWnd), so Shell_NotifyIconW works for real.
# Checked: the balloon is accepted, stay_alive keeps the icon after the window is closed (a second balloon
# still goes out), stay_alive(false) takes it away; with no shell the answers agree (notify == stay_alive).
import os, shutil, subprocess, sys, tempfile, time
from tscale import T, scratch_dir

FAILED = []
def check(name, cond, extra=""):
    print(("  OK    " if cond else "  FAIL  ") + name + ((" -- " + str(extra)[:300]) if (extra and not cond) else ""))
    if not cond:
        FAILED.append(name)

for tool in ("wine", "Xvfb"):
    if shutil.which(tool) is None:
        print("  SKIP  %s not installed" % tool)
        sys.exit(0)
exe = os.path.abspath(sys.argv[1])
n = next(i for i in range(300, 400) if not os.path.exists("/tmp/.X11-unix/X%d" % i) and not os.path.exists("/tmp/.X%d-lock" % i))
xvfb = subprocess.Popen(["Xvfb", ":%d" % n, "-screen", "0", "800x600x24"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
env = dict(os.environ, DISPLAY=":%d" % n, WINEPREFIX=os.environ.get("WINEPREFIX", os.path.expanduser("~/.wine-firn")), WINEDEBUG="-all")
for _ in range(100):
    if os.path.exists("/tmp/.X11-unix/X%d" % n):
        break
    time.sleep(0.1)
time.sleep(0.3)

def run(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True, env=env, timeout=T(90))
    return r.stdout.split()

try:
    subprocess.run(["wineserver", "-k"], env=env)
    time.sleep(1.5)
    # no shell of ours: whatever Wine's own session does, the answers must agree
    o0 = run(["wine", exe])
    check("without our shell: NOTIFY and ALIVE agree and the program ends", "END" in o0 and o0[o0.index("NOTIFY") + 1] == o0[o0.index("ALIVE") + 1], o0)
    subprocess.run(["wineserver", "-k"], env=env)
    time.sleep(1.5)
    o1 = run(["wine", "explorer", "/desktop=shell,800x600", exe])
    check("with the shell: balloon accepted (NOTIFY yes)", o1[:2] == ["NOTIFY", "yes"], o1)
    check("stay_alive(true) keeps the icon (ALIVE yes)", "ALIVE" in o1 and o1[o1.index("ALIVE") + 1] == "yes", o1)
    check("window closed, icon stays: second balloon goes out (NOTIFY2 yes)", "NOTIFY2" in o1 and o1[o1.index("NOTIFY2") + 1] == "yes", o1)
    check("stay_alive(false) is honestly false and the program ends", "ALIVE_OFF" in o1 and o1[o1.index("ALIVE_OFF") + 1] == "no" and "END" in o1, o1)
finally:
    subprocess.run(["wineserver", "-k"], env=env)
    xvfb.terminate()
print("winnotify (Windows): %d failed" % len(FAILED) if FAILED else "winnotify (Windows): all checks passed")
sys.exit(1 if FAILED else 0)
