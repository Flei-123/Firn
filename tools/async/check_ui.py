#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/async/check_ui.py <ui_probe> -- a window and the event loop in one
# thread, on an Xvfb of its own (python-xlib):
#
#   1. READY, then NET 200 (an HTTP answer), POST 42 (a worker thread's result)
#      and TIMER 1..3 arrive WITHOUT any window event: the UI wait is woken by
#      the loop's descriptor (window.wait_any_fd)
#   2. all of that came to the UI thread within about a second
#   3. the idle program does not spin: the CPU time of one quiet second
#   4. a window event (close) is still seen at once while the loop is idle
#   5. the loop did few rounds in all (it slept between the events)
import os, shutil, subprocess, sys, time

probe = sys.argv[1]
if not shutil.which("Xvfb"):
    print("  skip: no Xvfb"); sys.exit(0)
try:
    from Xlib import display, X, protocol
except ImportError:
    print("  skip: no python-xlib"); sys.exit(0)

disp = None
for n in range(180, 200):
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

def cpu_ticks(pid):
    with open("/proc/%d/stat" % pid) as f:
        parts = f.read().rsplit(")", 1)[1].split()
    return int(parts[11]) + int(parts[12])   # utime + stime

try:
    env = dict(os.environ, DISPLAY=":%d" % disp)
    p = subprocess.Popen([probe], env=env, stdout=subprocess.PIPE, text=True, bufsize=1)
    t0 = time.time()
    first = p.stdout.readline().strip()
    (ok if first == "READY" else fail)("the window and the loop started (%s)" % first)
    seen = []
    deadline = time.time() + 6
    want = {"NET 200", "POST 42", "TIMER 1", "TIMER 2", "TIMER 3"}
    while want - set(seen) and time.time() < deadline:
        line = p.stdout.readline().strip()
        if not line:
            break
        seen.append(line)
    dt = time.time() - t0
    missing = sorted(want - set(seen))
    (ok if not missing else fail)("network, worker thread and timers reached the UI thread with no window event (missing %s, saw %s)" % (missing, seen))
    (ok if dt < 2.5 else fail)("... within %.2f s" % dt)
    # order: the timers come in order
    timers = [s for s in seen if s.startswith("TIMER")]
    (ok if timers == ["TIMER 1", "TIMER 2", "TIMER 3"] else fail)("the timer fired three times, in order (%s)" % timers)
    time.sleep(0.4)
    c0 = cpu_ticks(p.pid)
    time.sleep(1.0)
    c1 = cpu_ticks(p.pid)
    (ok if c1 - c0 <= 4 else fail)("an idle second costs %d clock ticks of CPU (limit 4 = 4 %%)" % (c1 - c0))
    # a window event is seen at once
    d = display.Display(":%d" % disp)
    root = d.screen().root
    win = None
    for w in root.query_tree().children:
        if w.get_wm_name() == "Firn Async UI":
            win = w
    (ok if win is not None else fail)("the window exists")
    if win is not None:
        wm_protocols = d.intern_atom("WM_PROTOCOLS")
        wm_delete = d.intern_atom("WM_DELETE_WINDOW")
        ev = protocol.event.ClientMessage(window=win, client_type=wm_protocols,
                                          data=(32, [wm_delete, X.CurrentTime, 0, 0, 0]))
        t1 = time.time()
        win.send_event(ev, event_mask=0)
        d.flush()
        line = p.stdout.readline().strip()
        dt = time.time() - t1
        (ok if line == "CLOSED" and dt < 1.0 else fail)("closing the window is seen at once (%s after %.0f ms)" % (line, dt * 1000))
        rest = p.communicate(timeout=10)[0].split("\n")
        rest = [x.strip() for x in rest if x.strip()]
        rounds = [x for x in rest if x.startswith("ROUNDS")]
        done = "DONE" in rest
        (ok if done and p.returncode == 0 else fail)("the program ended cleanly (%s, exit %s)" % (rest, p.returncode))
        if rounds:
            n = int(rounds[0].split()[1])
            (ok if n < 200 else fail)("the loop slept between events: %d rounds in about 2 s" % n)
        else:
            fail("no ROUNDS line")
    else:
        p.kill()
finally:
    xvfb.terminate()
print("ui: %d failed" % bad)
sys.exit(1 if bad else 0)
