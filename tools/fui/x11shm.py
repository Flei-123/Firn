#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fui/x11shm.py <x11demo-program> -- MIT-SHM in lib/window/x11.fi (OpenPlan r23).
#
# The same demo window on four Xvfb setups; what is checked comes from the server
# (xwd: what really stands in the window) and from the kernel (strace: which System V
# shared memory calls the program made):
#   1. an ordinary Xvfb: the program makes a segment (shmget, shmat, shmctl IPC_RMID), the
#      window has content
#   2. FIRN_X11_NOSHM=1: no shmget at all, the SAME window pixels as in 1
#   3. Xvfb without the extension (-extension MIT-SHM): no shmget at all, the same pixels
#   4. an Xvfb in another IPC namespace (unshare --ipc): the server cannot attach the
#      segment, the program notices (an error reply to ShmAttach), lets go of it again and
#      paints the same pixels through PutImage
# Without Xvfb, xdotool, xwd, strace or unshare: SKIP (exit 0).
import os, shutil, struct, subprocess, sys, time

PROG = sys.argv[1]
for t in ("Xvfb", "xdotool", "xwd", "strace", "unshare"):
    if shutil.which(t) is None:
        print("SKIP: %s missing" % t)
        sys.exit(0)
bad = 0


def chk(name, ok, got):
    global bad
    if not ok:
        bad += 1
    print("  %s %s (%s)" % ("ok  " if ok else "FAIL", name, got), flush=True)


def free_display():
    for n in range(181, 260):
        if not os.path.exists("/tmp/.X%d-lock" % n) and not os.path.exists("/tmp/.X11-unix/X%d" % n):
            return n
    raise SystemExit("no free display")


def pixels(xwd):
    hs = struct.unpack(">I", xwd[:4])[0]
    nc = struct.unpack(">I", xwd[76:80])[0]
    w, h = struct.unpack(">II", xwd[16:24])
    return w, h, xwd[hs + nc * 12:]


def run(label, xvfb_prefix=(), xvfb_extra=(), env_extra=None):
    d = free_display()
    xv = subprocess.Popen(list(xvfb_prefix) + ["Xvfb", ":%d" % d, "-screen", "0", "1280x800x24", "-nolisten", "tcp"] + list(xvfb_extra),
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    env = dict(os.environ, DISPLAY=":%d" % d)
    env.update(env_extra or {})
    for _ in range(100):
        if subprocess.run(["xdotool", "getmouselocation"], env=env, capture_output=True).returncode == 0:
            break
        time.sleep(0.1)
    log = "/tmp/x11shm-%d-%s.strace" % (os.getpid(), label)
    demo = subprocess.Popen(["strace", "-f", "-e", "trace=shmget,shmat,shmctl,shmdt", "-o", log, PROG, "--anzeige=%d" % d],
                            env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    time.sleep(4)
    xwd = subprocess.run(["xwd", "-root", "-silent"], env=env, capture_output=True).stdout
    subprocess.run(["xdotool", "key", "Escape"], env=env)
    try:
        demo.wait(timeout=10)
    except subprocess.TimeoutExpired:
        demo.kill()
    xv.terminate()
    xv.wait()
    calls = open(log).read() if os.path.exists(log) else ""
    if os.path.exists(log):
        os.remove(log)
    w, h, px = pixels(xwd)
    return w, h, px, calls


def names(calls):
    out = []
    for l in calls.splitlines():
        for n in ("shmget", "shmat", "shmctl", "shmdt"):
            if " %s(" % n in l:
                out.append(n)
    return out


w, h, base, calls = run("shm")
n1 = names(calls)
chk("1 an ordinary Xvfb: segment made, attached, marked for removal", n1[:3] == ["shmget", "shmat", "shmctl"] and "IPC_RMID" in calls, " ".join(n1))
colours = len(set(base[i:i + 4] for i in range(0, len(base), 4 * 97)))
chk("1 the window has content (colours in a sample)", colours > 50, colours)
chk("1 the segment is let go of when the window closes", n1[-1:] == ["shmdt"], " ".join(n1))
w2, h2, p2, c2 = run("env", env_extra={"FIRN_X11_NOSHM": "1"})
chk("2 FIRN_X11_NOSHM=1: no shared memory call", names(c2) == [], " ".join(names(c2)))
chk("2 the same pixels", (w2, h2) == (w, h) and p2 == base, "%dx%d" % (w2, h2))
w3, h3, p3, c3 = run("noext", xvfb_extra=("-extension", "MIT-SHM"))
chk("3 a server without MIT-SHM: no shared memory call", names(c3) == [], " ".join(names(c3)))
chk("3 the same pixels", (w3, h3) == (w, h) and p3 == base, "%dx%d" % (w3, h3))
w4, h4, p4, c4 = run("ns", xvfb_prefix=("unshare", "--ipc"))
n4 = names(c4)
chk("4 another IPC namespace: the segment is tried and given up again", n4[:1] == ["shmget"] and "shmdt" in n4 and n4.count("shmget") == 1, " ".join(n4))
chk("4 the same pixels (PutImage bands)", (w4, h4) == (w, h) and p4 == base, "%dx%d" % (w4, h4))
print("x11shm: %d failed" % bad)
sys.exit(1 if bad else 0)
