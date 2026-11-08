#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os, shutil, subprocess, sys, time

PROG = sys.argv[1]
for tool in ("Xvfb", "xdotool"):
    if shutil.which(tool) is None:
        print("SKIP: %s missing" % tool)
        sys.exit(0)

errors = 0


def check(text, ok):
    global errors
    if not ok:
        errors += 1
    print("  %s %s" % ("ok   " if ok else "WRONG", text), flush=True)


def free_display():
    for n in range(200, 260):
        if not os.path.exists("/tmp/.X11-unix/X%d" % n) and not os.path.exists("/tmp/.X%d-lock" % n):
            return n
    raise SystemExit("no free display")


def windows(env, name):
    r = subprocess.run(["xdotool", "search", "--name", name], env=env, capture_output=True, text=True)
    return r.stdout.split()


def wait_for(env, name, present, seconds):
    end = time.time() + seconds
    while time.time() < end:
        if bool(windows(env, name)) == present:
            return True
        time.sleep(0.05)
    return False


n = free_display()
env = dict(os.environ, DISPLAY=":%d" % n)
env.pop("XAUTHORITY", None)
server = subprocess.Popen(["Xvfb", ":%d" % n, "-screen", "0", "1024x768x24", "-nolisten", "tcp", "-noreset"],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
try:
    for _ in range(100):
        if os.path.exists("/tmp/.X11-unix/X%d" % n):
            break
        time.sleep(0.05)
    prog = subprocess.Popen([PROG], env=env, stdout=subprocess.PIPE, text=True)
    check("main window appears", wait_for(env, "Live main", True, 10))
    check("child window appears", wait_for(env, "Live child", True, 10))
    check("both windows exist together", bool(windows(env, "Live main")) and bool(windows(env, "Live child")))
    child = windows(env, "Live child")[0]
    time.sleep(0.5)
    subprocess.run(["xdotool", "mousemove", "--window", child, "150", "80", "click", "1"], env=env)
    check("clicking the child's button closes the child window", wait_for(env, "Live child", False, 10))
    check("main window stays after the child closed", bool(windows(env, "Live main")))
    out, _ = prog.communicate(timeout=30)
    check("program exits with 0", prog.returncode == 0)
    check("output order: opened, child closed, main quit",
          out.split("\n")[:3] == ["child opened", "child closed", "main quit"])
    check("no windows left", not windows(env, "Live main") and not windows(env, "Live child"))
finally:
    server.terminate()
    server.wait()
print("APPWIN LIVE PASSED" if errors == 0 else "APPWIN LIVE FAILED (%d)" % errors)
sys.exit(1 if errors else 0)
