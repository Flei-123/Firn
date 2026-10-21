#!/usr/bin/env python3
"""UI comparison measurement harness.

Usage:
  measure.py --app flei --toolkit fui \
     --cmd "/path/app --theme {theme} --shot {shot}" \
     [--run-cmd "/path/app --theme {theme}"]   (interactive mode; default = --cmd without --shot)
     [--src dir_or_files ...]                  (lines of code, via loc.py)
     [--size-paths file_or_dir ...]            (binary / bundle size, bytes summed)
     [--runtime-size-paths ...]                (e.g. browser binary; separate row "Runtime")
     [--build-cmd "cargo build --release"]     (clean build is the caller's job; wall time)
     [--runs 5] [--idle-runs N] [--theme dark] [--input key:Tab|click:550,350]
     [--cwd DIR] [--shots-dir DIR] [--out DIR|json]

Output: results/<app>-<toolkit>.json with median/min/max for each metric.

Method (all under a private Xvfb 1100x700x24, no window manager, DPR 1):
 startup_ms      wall time from spawn of --cmd until <shot>.png exists, is a complete PNG, and
                 (when the process exits) it exits. Reported value = time to complete PNG.
 size_bytes      sum of file sizes of --size-paths.
 ram_pss_kb      --run-cmd started, 5 s idle, then sum of Pss from /proc/<pid>/smaps_rollup over the
                 whole process tree (spawned pid + all descendants; Web = browser process tree).
 cpu_idle_pct    utime+stime of the same tree between t=5 s and t=15 s (10 s) / 10 s * 100
                 (100 % = one full core).
 input_latency_ms  xdotool-like XTest event (libXtst, XFlush) -> first changed pixel in the root window,
                 polled with XGetImage in a tight loop (each poll ~1-3 ms for 1100x700). Baseline image
                 is taken after the screen is stable. Lower bound of real latency + up to one poll.
                 Runs without any visible change within 2 s are counted as 'no change'.
 build_s         wall time of --build-cmd (once).
 frames          optional hook: if the app writes JSON to $UIVGL_FRAMES
                 ({"scroll_ms":..,"resize_ms":..,"anim_ms":..}) it is included, else 'n/a' + reason.
"""
import argparse
import ctypes
import ctypes.util
import datetime
import json
import os
import re
import shlex
import shutil
import signal
import statistics
import subprocess
import sys
import tempfile
import time

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
HERE = os.environ.get("UIVGL_DIR", SCRIPTS)  # data/shots/results/apps root
sys.path.insert(0, SCRIPTS)
W, H = 1100, 700
CLK = os.sysconf("SC_CLK_TCK")


def stats(vals, nd=2):
    vals = [v for v in vals if v is not None]
    if not vals:
        return None
    return {"median": round(statistics.median(vals), nd), "min": round(min(vals), nd),
            "max": round(max(vals), nd), "n": len(vals), "runs": [round(v, nd) for v in vals]}


# ---------------------------------------------------------------- Xvfb
class Xvfb:
    def __init__(self):
        r, w = os.pipe()
        self.p = subprocess.Popen(
            ["Xvfb", "-displayfd", str(w), "-screen", "0", "%dx%dx24" % (W, H), "-nolisten", "tcp", "-ac"],
            pass_fds=(w,), stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, start_new_session=True)
        os.close(w)
        buf = b""
        t0 = time.time()
        while not buf.endswith(b"\n") and time.time() - t0 < 10:
            buf += os.read(r, 32)
        os.close(r)
        self.display = ":" + buf.decode().strip()

    def close(self):
        try:
            os.killpg(self.p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        self.p.wait()


# ---------------------------------------------------------------- process tree helpers
def children_map():
    cm = {}
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            with open("/proc/%s/stat" % d) as f:
                s = f.read()
            rest = s[s.rindex(")") + 2:].split()
            cm.setdefault(int(rest[1]), []).append(int(d))
        except (OSError, ValueError):
            pass
    return cm


def tree(pid):
    cm = children_map()
    out, stack = [], [pid]
    while stack:
        p = stack.pop()
        out.append(p)
        stack.extend(cm.get(p, []))
    return out


def pss_kb(pids):
    tot = 0
    for p in pids:
        try:
            with open("/proc/%d/smaps_rollup" % p) as f:
                for line in f:
                    if line.startswith("Pss:"):
                        tot += int(line.split()[1])
                        break
        except OSError:
            pass
    return tot


def cpu_ticks(pids):
    out = {}
    for p in pids:
        try:
            with open("/proc/%d/stat" % p) as f:
                s = f.read()
            rest = s[s.rindex(")") + 2:].split()
            out[p] = int(rest[11]) + int(rest[12])
        except (OSError, ValueError):
            pass
    return out


def kill_tree(proc):
    try:
        os.killpg(proc.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    try:
        proc.wait(timeout=5)
    except Exception:
        pass


# ---------------------------------------------------------------- X capture + input (ctypes)
class XImage(ctypes.Structure):
    _fields_ = [("width", ctypes.c_int), ("height", ctypes.c_int), ("xoffset", ctypes.c_int),
                ("format", ctypes.c_int), ("data", ctypes.c_void_p), ("byte_order", ctypes.c_int),
                ("bitmap_unit", ctypes.c_int), ("bitmap_bit_order", ctypes.c_int),
                ("bitmap_pad", ctypes.c_int), ("depth", ctypes.c_int), ("bytes_per_line", ctypes.c_int),
                ("bits_per_pixel", ctypes.c_int)]


class XConn:
    def __init__(self, display):
        self.x = ctypes.CDLL("libX11.so.6")
        self.t = ctypes.CDLL("libXtst.so.6")
        self.x.XOpenDisplay.restype = ctypes.c_void_p
        self.x.XOpenDisplay.argtypes = [ctypes.c_char_p]
        self.x.XDefaultRootWindow.restype = ctypes.c_ulong
        self.x.XDefaultRootWindow.argtypes = [ctypes.c_void_p]
        self.x.XGetImage.restype = ctypes.POINTER(XImage)
        self.x.XGetImage.argtypes = [ctypes.c_void_p, ctypes.c_ulong, ctypes.c_int, ctypes.c_int,
                                     ctypes.c_uint, ctypes.c_uint, ctypes.c_ulong, ctypes.c_int]
        self.x.XDestroyImage.argtypes = [ctypes.POINTER(XImage)]
        self.x.XStringToKeysym.restype = ctypes.c_ulong
        self.x.XStringToKeysym.argtypes = [ctypes.c_char_p]
        self.x.XKeysymToKeycode.restype = ctypes.c_ubyte
        self.x.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
        self.x.XFlush.argtypes = [ctypes.c_void_p]
        self.x.XSync.argtypes = [ctypes.c_void_p, ctypes.c_int]
        self.t.XTestFakeKeyEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        self.t.XTestFakeButtonEvent.argtypes = [ctypes.c_void_p, ctypes.c_uint, ctypes.c_int, ctypes.c_ulong]
        self.t.XTestFakeMotionEvent.argtypes = [ctypes.c_void_p, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                                                ctypes.c_ulong]
        self.d = self.x.XOpenDisplay(display.encode())
        if not self.d:
            raise RuntimeError("XOpenDisplay failed for " + display)
        self.root = self.x.XDefaultRootWindow(self.d)

    def grab(self):
        im = self.x.XGetImage(self.d, self.root, 0, 0, W, H, 0xFFFFFFFF, 2)  # ZPixmap
        if not im:
            return b""
        n = im.contents.bytes_per_line * im.contents.height
        data = ctypes.string_at(im.contents.data, n)
        self.x.XDestroyImage(im)
        return data

    def send(self, spec):
        """spec: key:Tab | click:x,y | move:x,y | wheel:N"""
        kind, _, arg = spec.partition(":")
        if kind == "key":
            kc = self.x.XKeysymToKeycode(self.d, self.x.XStringToKeysym(arg.encode()))
            self.t.XTestFakeKeyEvent(self.d, kc, 1, 0)
            self.t.XTestFakeKeyEvent(self.d, kc, 0, 0)
        elif kind == "wheel":  # wheel:N  N>0 = N notches down (button 5), N<0 up (button 4)
            n = int(arg)
            b = 5 if n > 0 else 4
            for _ in range(abs(n)):
                self.t.XTestFakeButtonEvent(self.d, b, 1, 0)
                self.t.XTestFakeButtonEvent(self.d, b, 0, 0)
        elif kind in ("click", "move"):
            xx, yy = [int(v) for v in arg.split(",")]
            self.t.XTestFakeMotionEvent(self.d, 0, xx, yy, 0)
            if kind == "click":
                self.t.XTestFakeButtonEvent(self.d, 1, 1, 0)
                self.t.XTestFakeButtonEvent(self.d, 1, 0, 0)
        else:
            raise ValueError("bad input spec " + spec)
        self.x.XFlush(self.d)


def valid_png(path):
    try:
        with open(path, "rb") as f:
            b = f.read()
        return len(b) > 100 and b[:8] == b"\x89PNG\r\n\x1a\n" and b[-12:-8] == b"\x00\x00\x00\x00" and b[-8:-4] == b"IEND"
    except OSError:
        return False


def fmt(cmd, shot, theme):
    return cmd.replace("{shot}", shot).replace("{theme}", theme)


def spawn(cmd, display, cwd, env_extra=None):
    env = dict(os.environ)
    env["DISPLAY"] = display
    env["GDK_BACKEND"] = "x11"
    env["QT_QPA_PLATFORM"] = "xcb"
    env["WINIT_UNIX_BACKEND"] = "x11"
    env.pop("WAYLAND_DISPLAY", None)
    if env_extra:
        env.update(env_extra)
    return subprocess.Popen(cmd, shell=True, cwd=cwd, env=env, stdout=subprocess.DEVNULL,
                            stderr=subprocess.DEVNULL, start_new_session=True)


# ---------------------------------------------------------------- measurements
def m_startup(a, xv, work):
    vals = []
    shot = os.path.join(work, "startup.png")
    for i in range(a.runs):
        if os.path.exists(shot):
            os.remove(shot)
        cmd = fmt(a.cmd, shot, a.theme)
        t0 = time.perf_counter()
        p = spawn(cmd, xv.display, a.cwd)
        t_png = None
        while time.perf_counter() - t0 < a.timeout:
            if t_png is None and os.path.exists(shot) and valid_png(shot):
                t_png = time.perf_counter() - t0
                break
            if p.poll() is not None and not os.path.exists(shot):
                break
            time.sleep(0.002)
        kill_tree(p)
        vals.append(t_png * 1000 if t_png else None)
        if i == 0 and t_png is None:
            return None, "no valid PNG written within %ds" % a.timeout
    return stats(vals, 1), None


def m_idle(a, xv, work):
    cmd = fmt(a.run_cmd, os.path.join(work, "unused.png"), a.theme)
    pss, cpu = [], []
    for i in range(a.idle_runs):
        p = spawn(cmd, xv.display, a.cwd)
        time.sleep(5)
        if p.poll() is not None:
            kill_tree(p)
            return None, None, "interactive process exited early (code %s)" % p.returncode
        pids = tree(p.pid)
        t0 = cpu_ticks(pids)
        pss.append(pss_kb(pids))
        s = time.time()
        time.sleep(10)
        t1 = cpu_ticks(pids)
        el = time.time() - s
        dt = sum(max(0, t1[k] - t0.get(k, t1[k])) for k in t1)
        cpu.append(dt / CLK / el * 100.0)
        kill_tree(p)
    return stats(pss, 0), stats(cpu, 2), None


def m_latency(a, xv, work):
    cmd = fmt(a.run_cmd, os.path.join(work, "unused.png"), a.theme)
    p = spawn(cmd, xv.display, a.cwd)
    x = XConn(xv.display)
    # wait until the app has drawn something (root image differs from the empty one), then settle
    g0 = x.grab()
    t_lim = time.time() + 20
    while time.time() < t_lim and x.grab() == g0 and p.poll() is None:
        time.sleep(0.05)
    time.sleep(a.settle)
    if p.poll() is not None:
        kill_tree(p)
        return None, "interactive process exited early"
    # screen stable? (noise check: caret blink / animation)
    base = x.grab()
    noisy = 0
    t_end = time.time() + 1.0
    while time.time() < t_end:
        g = x.grab()
        if g != base:
            noisy += 1
            base = g
        time.sleep(0.02)
    vals, nochange = [], 0
    inputs = a.input.split(";")
    for i in range(a.runs):
        # wait until the screen is unchanged for 300 ms (max 5 s)
        stable_since, last = time.time(), x.grab()
        t_lim = time.time() + 5
        while time.time() - stable_since < 0.3 and time.time() < t_lim:
            g = x.grab()
            if g != last:
                last, stable_since = g, time.time()
            time.sleep(0.01)
        base = last
        t0 = time.perf_counter()
        x.send(inputs[i % len(inputs)])
        got = None
        while time.perf_counter() - t0 < 2.0:
            g = x.grab()
            if g != base:
                got = (time.perf_counter() - t0) * 1000
                break
        if got is None:
            nochange += 1
        else:
            vals.append(got)
        time.sleep(0.3)
    kill_tree(p)
    note = None
    if noisy > 3:
        note = "screen not idle-stable before input (%d changes/1 s): values may be too low" % noisy
    if nochange:
        note = (note + "; " if note else "") + "%d of %d inputs gave no visible change" % (nochange, a.runs)
    s = stats(vals, 1)
    return s, note


def m_frames(a, xv, work):
    fp = os.path.join(work, "frames.json")
    cmd = fmt(a.run_cmd, os.path.join(work, "unused.png"), a.theme)
    p = spawn(cmd, xv.display, a.cwd, {"UIVGL_FRAMES": fp})
    time.sleep(6)
    kill_tree(p)
    if os.path.exists(fp):
        try:
            with open(fp) as f:
                return json.load(f), None
        except ValueError:
            return None, "UIVGL_FRAMES file not valid JSON"
    return None, "app has no frame hook (writes no $UIVGL_FRAMES JSON)"


def dir_size(paths):
    tot = 0
    for p in paths:
        if os.path.isdir(p):
            for r, _, fs in os.walk(p):
                for f in fs:
                    try:
                        tot += os.path.getsize(os.path.join(r, f))
                    except OSError:
                        pass
        elif os.path.exists(p):
            tot += os.path.getsize(p)
    return tot


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--app", required=True)
    ap.add_argument("--toolkit", required=True)
    ap.add_argument("--cmd", required=True)
    ap.add_argument("--run-cmd")
    ap.add_argument("--src", nargs="*", default=[])
    ap.add_argument("--size-paths", nargs="*", default=[])
    ap.add_argument("--runtime-size-paths", nargs="*", default=[])
    ap.add_argument("--build-cmd")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--idle-runs", type=int)
    ap.add_argument("--theme", default="dark")
    ap.add_argument("--input", default="key:Tab", help="';'-separated XTest specs used round-robin")
    ap.add_argument("--settle", type=float, default=2.0)
    ap.add_argument("--timeout", type=int, default=60)
    ap.add_argument("--cwd", default=None)
    ap.add_argument("--out", default="json", help="'json' (= results/ dir) or a directory")
    ap.add_argument("--only", default="", help="comma list: startup,idle,latency,frames,build,size,lines")
    a = ap.parse_args()
    a.idle_runs = a.idle_runs or a.runs
    if not a.run_cmd:
        a.run_cmd = re.sub(r"\s*--shot\s+\S*\{shot\}\S*", "", a.cmd)
    only = set(x for x in a.only.split(",") if x) or {"startup", "idle", "latency", "frames", "build", "size", "lines"}
    outdir = os.path.join(HERE, "results") if a.out == "json" else a.out
    os.makedirs(outdir, exist_ok=True)
    work = tempfile.mkdtemp(prefix="uivgl-")
    res = {"app": a.app, "toolkit": a.toolkit, "theme": a.theme, "cmd": a.cmd, "run_cmd": a.run_cmd,
           "date": datetime.datetime.now().isoformat(timespec="seconds"), "runs": a.runs,
           "load_avg": os.getloadavg()[0], "screen": "%dx%d" % (W, H), "notes": []}
    xv = None
    try:
        if "lines" in only and a.src:
            import loc
            paths = loc.collect(a.src, loc.EXT)
            rows = {p: loc.count_file(p) for p in paths}
            res["lines"] = {"total": sum(rows.values()), "files": rows}
        if "size" in only and a.size_paths:
            res["size_bytes"] = dir_size(a.size_paths)
        if a.runtime_size_paths:
            res["runtime_size_bytes"] = dir_size(a.runtime_size_paths)
        if "build" in only and a.build_cmd:
            t0 = time.time()
            r = subprocess.run(a.build_cmd, shell=True, cwd=a.cwd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            res["build_s"] = round(time.time() - t0, 1)
            res["build_rc"] = r.returncode
        xv = Xvfb()
        res["display"] = xv.display
        if "startup" in only:
            s, err = m_startup(a, xv, work)
            res["startup_ms"] = s
            if err:
                res["notes"].append("startup: " + err)
        if "idle" in only:
            pss, cpu, err = m_idle(a, xv, work)
            res["ram_pss_kb"], res["cpu_idle_pct"] = pss, cpu
            if err:
                res["notes"].append("idle: " + err)
        if "latency" in only:
            s, note = m_latency(a, xv, work)
            res["input_latency_ms"] = s
            if note:
                res["notes"].append("latency: " + note)
        if "frames" in only:
            f, why = m_frames(a, xv, work)
            res["frames"] = f if f else "n/a"
            if why:
                res["frames_reason"] = why
    finally:
        if xv:
            xv.close()
        shutil.rmtree(work, ignore_errors=True)
    path = os.path.join(outdir, "%s-%s.json" % (a.app, a.toolkit))
    with open(path, "w") as f:
        json.dump(res, f, indent=1)
    print(json.dumps({k: (v if not isinstance(v, dict) or "median" not in v else v["median"])
                      for k, v in res.items() if k not in ("cmd", "run_cmd", "lines")}, indent=1))
    if "lines" in res:
        print("lines:", res["lines"]["total"])
    print("->", path)


if __name__ == "__main__":
    main()
