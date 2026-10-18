#!/usr/bin/env python3
"""Repeatable on-device measurement harness for the Firn vs Kotlin Android benchmark apps.

Runs on a PC with adb (HOLO-LAPTOP-3) and a USB-connected phone (Xiaomi 14T Pro).
Stdlib only. Every phase takes the shared phone lock (%TEMP%/phone.lock) first.

  python measure.py --phase scroll|startup|cpunet|energy|all [--app kotlin|firn|both]
                    [--runs 5] [--scroll-s 10] [--energy-s 300] [--serial SERIAL] [--out DIR]

Phases (each is one lock slot, <= 10 min):
  scroll   autoscroll list, >=5 runs x >=10 s. SurfaceFlinger latency polled and merged (ring buffer
           is ~1 s), per-run p50/p95/p99/max, frames >8.5 ms (missed 120 Hz vsync, 8.33+0.2 tolerance),
           >20 ms; median over runs. Kotlin additionally: gfxinfo summary + framestats.
  startup  cold start x5 (am start -W), PSS/RSS breakdown x3 (dumpsys meminfo).
  cpunet   cpu lab (primes/sort/mandel, ms + checksums) and net lab, 3 runs each.
  energy   one app per slot (--app kotlin|firn): autoscroll for --energy-s seconds. batterystats --reset,
           battery temp/level/charge counter, thermalservice temperatures before/after and every 30 s,
           app CPU time (/proc/<pid>/stat), batterystats per-uid mAh estimate.
Results: <out>/<phase>_<app>_<timestamp>.json + one-line-per-metric summary on stdout.
Notes: when USB-charging, charge counter / current_now are NOT usable as drain (flagged "plugged");
the batterystats model estimate (mAh) and CPU time are the comparable numbers.
"""
import argparse, json, os, re, statistics, subprocess, sys, tempfile, time, datetime

APPS = {
    "kotlin": {"pkg": "com.fleitec.bench.kotlin", "act": "com.fleitec.bench.kotlin/.MainActivity"},
    "firn": {"pkg": "com.fleitec.bench.firn", "act": "com.fleitec.bench.firn/android.app.NativeActivity"},
}
DEFAULT_SERIAL = "LVSGQWMN5XGYSGJZ"
LOCK = os.path.join(tempfile.gettempdir(), "phone.lock")
LOCK_MAX_AGE_S = 600
VSYNC_120_MS = 8.333
OVER_VSYNC_MS = VSYNC_120_MS + 0.2
OVER_20_MS = 20.0
WHO = "fui-beat-kotlin-messharness"

ADB = os.environ.get("ADB") or os.path.join(os.path.expanduser("~"), "adbtools", "platform-tools", "adb.exe")
SERIAL = DEFAULT_SERIAL


def adb(*args, timeout=60, check=False):
    for attempt in (1, 2):
        try:
            p = subprocess.run([ADB, "-s", SERIAL, *args], capture_output=True, text=True, timeout=timeout,
                               encoding="utf-8", errors="replace")
            break
        except subprocess.TimeoutExpired:
            print("WARN adb timeout (%ds, attempt %d): %s" % (timeout, attempt, " ".join(args)[:80]), flush=True)
            if attempt == 2:
                return ""
    if check and p.returncode != 0:
        raise RuntimeError("adb %s failed: %s" % (" ".join(args), p.stderr.strip()))
    return p.stdout.replace("\r", "")


def sh(cmd, timeout=60):
    return adb("shell", cmd, timeout=timeout)


# ---------- lock ----------
def lock_acquire(wait_max_s=1800):
    t0 = time.time()
    while True:
        if os.path.exists(LOCK):
            age = time.time() - os.path.getmtime(LOCK)
            if age >= LOCK_MAX_AGE_S:
                break  # stale
            if time.time() - t0 > wait_max_s:
                raise RuntimeError("phone.lock held too long: " + open(LOCK).read().strip())
            print("phone.lock held (%s, %ds old) - waiting" % (open(LOCK).read().strip(), age), flush=True)
            time.sleep(30)
        else:
            break
    lock_touch()


def lock_touch():
    with open(LOCK, "w") as f:
        f.write("%s %s" % (WHO, datetime.datetime.now().isoformat(timespec="seconds")))


def lock_release():
    try:
        os.remove(LOCK)
    except FileNotFoundError:
        pass


# ---------- helpers ----------
def pct(sorted_vals, p):
    if not sorted_vals:
        return None
    return sorted_vals[min(len(sorted_vals) - 1, int(p * len(sorted_vals)))]


def med(vals):
    vals = [v for v in vals if v is not None]
    return round(statistics.median(vals), 3) if vals else None


def prep_device():
    sh("input keyevent KEYCODE_WAKEUP")
    sh("svc power stayon true")
    sh("settings put global window_animation_scale 0; settings put global transition_animation_scale 0; "
       "settings put global animator_duration_scale 1")


def start(app, extra="", wait=True):
    a = APPS[app]
    sh("am force-stop " + a["pkg"])
    time.sleep(1)
    adb("logcat", "-c", timeout=20)
    out = sh("am start %s -n %s %s" % ("-W" if wait else "", a["act"], extra))
    return out


def bench_lines(pat=""):
    out = adb("logcat", "-d", "-s", "BENCH:I")
    return [re.sub(r"^.*BENCH\s*:\s*", "", l) for l in out.splitlines() if "BENCH" in l and re.search(pat, l)]


def wait_bench(pat, tmax):
    for _ in range(tmax):
        time.sleep(1)
        l = bench_lines(pat)
        if l:
            return l[-1]
    return None


def find_layers(pkg):
    out = sh("dumpsys SurfaceFlinger --list")
    found = []
    for l in out.splitlines():
        if pkg in l and not re.search(r"Background|Bounds|Task=|Surface\(", l):
            m = re.search(r"(%s[^\s{}]*?#\d+)" % re.escape(pkg), l)
            if m and m.group(1) not in found:
                found.append(m.group(1))
    return found


def sf_poll(layer):
    """Return (list of actualPresentTime ns, refresh_period_ns)."""
    out = sh("dumpsys SurfaceFlinger --latency '%s'" % layer)
    lines = out.splitlines()
    if not lines:
        return [], 0
    try:
        period = int(lines[0].strip())
    except ValueError:
        period = 0
    vals = []
    for ln in lines[1:]:
        f = ln.split()
        if len(f) >= 3 and f[1].isdigit():
            v = int(f[1])
            if 0 < v < 9e18:
                vals.append(v)
    return vals, period


def interval_stats(ts_sorted, breaks):
    d = []
    for i in range(1, len(ts_sorted)):
        if ts_sorted[i] in breaks:
            continue
        d.append((ts_sorted[i] - ts_sorted[i - 1]) / 1e6)
    gaps = [x for x in d if x > 100]
    d = [x for x in d if x <= 100]
    s = sorted(d)
    if len(s) < 5:
        return {"frames": len(ts_sorted), "error": "too few samples"}
    span_ms = (ts_sorted[-1] - ts_sorted[0]) / 1e6
    return {
        "frames": len(ts_sorted), "span_ms": round(span_ms), "fps": round(len(d) * 1000.0 / sum(d), 1),
        "p50": round(pct(s, 0.50), 2), "p95": round(pct(s, 0.95), 2), "p99": round(pct(s, 0.99), 2),
        "max": round(s[-1], 2), "over_vsync": sum(1 for x in d if x > OVER_VSYNC_MS),
        "over_20ms": sum(1 for x in d if x > OVER_20_MS), "n_intervals": len(d),
        "stalls_gt100ms": len(gaps), "stall_max_ms": round(max(gaps), 1) if gaps else 0,
        "pct_over_vsync": round(100.0 * sum(1 for x in d if x > OVER_VSYNC_MS) / len(d), 2),
        "pct_over_20ms": round(100.0 * sum(1 for x in d if x > OVER_20_MS) / len(d), 2),
    }


def gfxinfo_summary(pkg):
    out = sh("dumpsys gfxinfo %s" % pkg)
    r = {}
    for key, pat in (("total_frames", r"Total frames rendered:\s*(\d+)"), ("janky", r"Janky frames:\s*(\d+)"),
                     ("janky_pct", r"Janky frames:\s*\d+\s*\(([\d.]+)%\)"),
                     ("p50", r"50th percentile:\s*(\d+)ms"), ("p90", r"90th percentile:\s*(\d+)ms"),
                     ("p95", r"95th percentile:\s*(\d+)ms"), ("p99", r"99th percentile:\s*(\d+)ms")):
        m = re.search(pat, out)
        if m:
            r[key] = float(m.group(1))
    return r


def gfx_framestats(pkg):
    out = sh("dumpsys gfxinfo %s framestats" % pkg)
    rows, hdr = [], None
    inb = False
    for l in out.splitlines():
        if l.startswith("---PROFILEDATA---"):
            inb = not inb
            continue
        if not inb:
            continue
        if l.startswith("Flags"):
            hdr = l.rstrip(",").split(",")
            continue
        f = l.rstrip(",").split(",")
        if hdr and len(f) >= len(hdr) and f[0] == "0":
            rows.append(dict(zip(hdr, f)))
    d = []
    for r in rows:
        try:
            d.append((int(r["FrameCompleted"]) - int(r["IntendedVsync"])) / 1e6)
        except (KeyError, ValueError):
            pass
    d = [x for x in d if 0 < x < 1000]
    return d


# ---------- phase: scroll ----------
def scroll_one(app, scroll_s, warm_s=3):
    a = APPS[app]
    start(app, "--es tab list --es theme dark --ei autoscroll 1")
    time.sleep(warm_s)
    layers = find_layers(a["pkg"])
    layer = None
    best = 0
    for l in layers:
        v, _ = sf_poll(l)
        if len(v) > best:
            best, layer = len(v), l
    if not layer:
        return {"error": "no SurfaceFlinger layer", "layers": layers}
    if app == "kotlin":
        sh("dumpsys gfxinfo %s reset" % a["pkg"])
    seen, breaks = set(), set()
    prev_max = None
    t_end = time.time() + scroll_s
    polls = 0
    while time.time() < t_end:
        v, period = sf_poll(layer)
        polls += 1
        if v:
            if prev_max is not None and min(v) > prev_max:
                breaks.add(min(v))  # no overlap with previous poll: interval unknown
            seen.update(v)
            prev_max = max(v)
        time.sleep(0.3)
    res = interval_stats(sorted(seen), breaks)
    res["layer"] = layer
    res["polls"] = polls
    res["breaks"] = len(breaks)
    if app == "kotlin":
        res["gfxinfo"] = gfxinfo_summary(a["pkg"])
        fs = sorted(gfx_framestats(a["pkg"]))
        if fs:
            res["framestats_ms"] = {"n": len(fs), "p50": round(pct(fs, .5), 2), "p95": round(pct(fs, .95), 2),
                                    "p99": round(pct(fs, .99), 2), "max": round(fs[-1], 2)}
    sh("am force-stop " + a["pkg"])
    return res


def phase_scroll(app, runs, scroll_s):
    runs_out = []
    for i in range(runs):
        lock_touch()
        r = scroll_one(app, scroll_s)
        print("scroll %s run %d: %s" % (app, i + 1, json.dumps(r)), flush=True)
        runs_out.append(r)
    ok = [r for r in runs_out if "error" not in r]
    keys = ("fps", "p50", "p95", "p99", "max", "over_vsync", "over_20ms", "pct_over_vsync", "pct_over_20ms",
            "stalls_gt100ms")
    summary = {k: med([r.get(k) for r in ok]) for k in keys}
    summary["runs_ok"] = len(ok)
    summary["frames_total"] = sum(r["frames"] for r in ok)
    if app == "kotlin":
        summary["gfxinfo_janky_pct_median"] = med([r.get("gfxinfo", {}).get("janky_pct") for r in ok])
        summary["gfxinfo_p99_ms_median"] = med([r.get("gfxinfo", {}).get("p99") for r in ok])
        summary["framestats_p95_median"] = med([r.get("framestats_ms", {}).get("p95") for r in ok])
        summary["framestats_p99_median"] = med([r.get("framestats_ms", {}).get("p99") for r in ok])
    return {"runs": runs_out, "median": summary, "scroll_s": scroll_s}


# ---------- phase: startup (cold start + PSS) ----------
def parse_meminfo(txt):
    r = {}
    m = re.search(r"TOTAL PSS:\s*(\d+)", txt) or re.search(r"^\s*TOTAL\s+(\d+)", txt, re.M)
    if m:
        r["pss_kb"] = int(m.group(1))
    m = re.search(r"TOTAL RSS:\s*(\d+)", txt)
    if m:
        r["rss_kb"] = int(m.group(1))
    for key, pat in (("java_kb", r"Java Heap:\s+(\d+)"), ("native_kb", r"Native Heap:\s+(\d+)"),
                     ("graphics_kb", r"Graphics:\s+(\d+)"), ("code_kb", r"Code:\s+(\d+)")):
        m = re.search(pat, txt)
        if m:
            r[key] = int(m.group(1))
    return r


def phase_startup(app, runs):
    a = APPS[app]
    cold = []
    for i in range(runs):
        lock_touch()
        out = start(app, "--es tab list --es theme dark")
        tt = re.search(r"TotalTime:\s*(\d+)", out)
        wt = re.search(r"WaitTime:\s*(\d+)", out)
        time.sleep(6)
        ready = [re.search(r"ms=(\d+)", l) for l in bench_lines("ready")]
        cold.append({"total_ms": int(tt.group(1)) if tt else None, "wait_ms": int(wt.group(1)) if wt else None,
                     "ready_ms": int(ready[-1].group(1)) if ready and ready[-1] else None})
        print("cold %s run %d: %s" % (app, i + 1, cold[-1]), flush=True)
    mem = []
    for i in range(3):
        lock_touch()
        start(app, "--es tab list --es theme dark")
        time.sleep(6)
        mem.append(parse_meminfo(sh("dumpsys meminfo " + a["pkg"])))
        print("mem %s run %d: %s" % (app, i + 1, mem[-1]), flush=True)
    sh("am force-stop " + a["pkg"])
    return {"cold": cold, "mem": mem,
            "median": {"cold_total_ms": med([c["total_ms"] for c in cold]),
                       "cold_ready_ms": med([c["ready_ms"] for c in cold]),
                       **{k: med([m.get(k) for m in mem]) for k in ("pss_kb", "rss_kb", "java_kb", "native_kb", "graphics_kb")}}}


# ---------- phase: cpu + net ----------
def phase_cpunet(app, runs=3):
    cpu, net = [], []
    for i in range(runs):
        lock_touch()
        start(app, "--es tab lab --es lab net --es theme dark --ez autorun true")
        l = wait_bench(r"\bnet\b", 40)
        m = re.search(r"status=(\d+) bytes=(\d+) ms=(\d+)", l or "")
        net.append({"status": int(m.group(1)), "ms": int(m.group(3))} if m else {"raw": l})
    for i in range(runs):
        lock_touch()
        start(app, "--es tab lab --es lab cpu --es theme dark --ez autorun true")
        l = wait_bench(r"\bcpu primes", 120)
        time.sleep(1)
        pure = [x for x in bench_lines(r"cpu_pure")]
        r = {}
        for k in ("primes", "sort", "mandel"):
            m = re.search(k + r"=(\d+):(\d+)", l or "")
            if m:
                r[k + "_ms"], r[k + "_sum"] = int(m.group(1)), int(m.group(2))
            m = re.search(k + r"=(\d+)", pure[-1]) if pure else None
            if m:
                r[k + "_pure_ms"] = int(m.group(1))
        cpu.append(r or {"raw": l})
        print("cpu %s run %d: %s" % (app, i + 1, cpu[-1]), flush=True)
    sh("am force-stop " + APPS[app]["pkg"])
    keys = ("primes_ms", "sort_ms", "mandel_ms", "primes_pure_ms", "sort_pure_ms", "mandel_pure_ms")
    return {"cpu": cpu, "net": net,
            "median": {**{k: med([c.get(k) for c in cpu]) for k in keys}, "net_ms": med([n.get("ms") for n in net])}}


# ---------- phase: energy / thermal ----------
def battery_state():
    t = sh("dumpsys battery")
    g = lambda pat: (re.search(pat, t) or [None, None])[1]
    r = {"level": g(r"level:\s*(\d+)"), "temp_c": None, "charge_counter_uah": g(r"Charge counter:\s*(-?\d+)"),
         "usb": g(r"USB powered:\s*(\w+)"), "ac": g(r"AC powered:\s*(\w+)"), "wireless": g(r"Wireless powered:\s*(\w+)")}
    tt = g(r"temperature:\s*(-?\d+)")
    if tt is not None:
        r["temp_c"] = int(tt) / 10.0
    for k in ("level", "charge_counter_uah"):
        if r[k] is not None:
            r[k] = int(r[k])
    r["plugged"] = any(r.get(k) == "true" for k in ("usb", "ac", "wireless"))
    return r


def thermal_state():
    t = sh("dumpsys thermalservice", timeout=30)
    r = {}
    m = re.search(r"Thermal Status:\s*(\d+)", t)
    if m:
        r["status"] = int(m.group(1))
    sect = t.split("Current temperatures from HAL:")[-1] if "Current temperatures from HAL:" in t else t
    sect = sect.split("Cached temperatures:")[0]
    temps = {}
    for m in re.finditer(r"Temperature\{mValue=(-?[\d.]+), mType=(\d+), mName=([^,}]+)", sect):
        temps[m.group(3)] = float(m.group(1))
    r["temps"] = temps
    return r


def pid_cpu_ms(pkg):
    pid = sh("pidof " + pkg).strip().split()
    if not pid:
        return None
    st = sh("cat /proc/%s/stat" % pid[0])
    try:
        f = st[st.rindex(")") + 2:].split()
        return (int(f[11]) + int(f[12])) * 10  # utime+stime in jiffies (100 Hz) -> ms
    except (ValueError, IndexError):
        return None


def current_now():
    c = sh("cat /sys/class/power_supply/battery/current_now").strip()
    v = sh("cat /sys/class/power_supply/battery/voltage_now").strip()
    return (int(c) if re.fullmatch(r"-?\d+", c) else None, int(v) if re.fullmatch(r"-?\d+", v) else None)


def parse_batterystats(txt, pkg):
    uid = None
    m = re.search(r"userId=(\d+)", sh("dumpsys package %s | grep userId" % pkg))
    if m:
        uid = int(m.group(1))
    r = {"uid": uid}
    m = re.search(r"Computed drain:\s*([\d.]+)", txt)
    if m:
        r["computed_drain_mah"] = float(m.group(1))
    m = re.search(r"Capacity:\s*(\d+)", txt)
    if m:
        r["capacity_mah"] = int(m.group(1))
    if uid is not None:
        au = "u0a%d" % (uid - 10000) if uid >= 10000 else str(uid)
        m = re.search(r"Uid %s:\s*([\d.]+)(.*)" % au, txt)
        if m:
            r["app_mah"] = float(m.group(1))
            r["app_detail"] = m.group(2).strip()[:200]
    m = re.search(r"^\s*Screen:\s*([\d.]+)", txt, re.M)
    if m:
        r["screen_mah"] = float(m.group(1))
    m = re.search(r"^\s*Cpu:\s*([\d.]+)", txt, re.M)
    if m:
        r["cpu_mah_all"] = float(m.group(1))
    return r


def phase_energy(app, energy_s, outdir):
    a = APPS[app]
    prev_bright = sh("settings get system screen_brightness").strip()
    prev_mode = sh("settings get system screen_brightness_mode").strip()
    sh("settings put system screen_brightness_mode 0; settings put system screen_brightness 128")
    try:
        sh("am force-stop " + a["pkg"])
        time.sleep(2)
        sh("dumpsys batterystats --reset", timeout=60)
        sh("dumpsys batterystats --enable full-wake-history")
        before = {"battery": battery_state(), "thermal": thermal_state()}
        start(app, "--es tab list --es theme dark --ei autoscroll 1")
        time.sleep(3)
        cpu0 = pid_cpu_ms(a["pkg"])
        t0 = time.time()
        samples = []
        while time.time() - t0 < energy_s:
            lock_touch()
            b, th = battery_state(), thermal_state()
            c, v = current_now()
            samples.append({"t_s": round(time.time() - t0), "temp_c": b["temp_c"], "level": b["level"],
                            "cur_uA": c, "volt_uV": v, "cpu_ms": pid_cpu_ms(a["pkg"]), "skin": th["temps"].get("skin")
                            if "skin" in th["temps"] else None, "hot": max(th["temps"].values()) if th["temps"] else None})
            time.sleep(max(0, 30 - 1.5))
        cpu1 = pid_cpu_ms(a["pkg"])
        elapsed = time.time() - t0
        after = {"battery": battery_state(), "thermal": thermal_state()}
        bs = sh("dumpsys batterystats", timeout=120)
        with open(os.path.join(outdir, "batterystats_%s_%s.txt" % (app, time.strftime("%Y%m%d-%H%M%S"))), "w",
                  encoding="utf-8") as f:
            f.write(bs)
        stats = parse_batterystats(bs, a["pkg"])
        sh("am force-stop " + a["pkg"])
    finally:
        if prev_mode in ("0", "1"):
            sh("settings put system screen_brightness_mode " + prev_mode)
        if prev_bright.isdigit():
            sh("settings put system screen_brightness " + prev_bright)
    res = {"elapsed_s": round(elapsed), "before": before, "after": after, "samples": samples, "batterystats": stats,
           "app_cpu_ms": (cpu1 - cpu0) if cpu0 is not None and cpu1 is not None else None}
    plugged = before["battery"]["plugged"] or after["battery"]["plugged"]
    res["plugged"] = plugged
    tb, ta = before["battery"]["temp_c"], after["battery"]["temp_c"]
    res["battery_temp_delta_c"] = round(ta - tb, 1) if tb is not None and ta is not None else None
    cb, ca = before["battery"]["charge_counter_uah"], after["battery"]["charge_counter_uah"]
    if cb is not None and ca is not None:
        res["charge_counter_delta_mah"] = round((ca - cb) / 1000.0, 2)
        res["drain_mah_5min_real"] = None if plugged else round((cb - ca) / 1000.0, 2)
    if res["app_cpu_ms"] is not None:
        res["app_cpu_pct_of_one_core"] = round(100.0 * res["app_cpu_ms"] / (elapsed * 1000.0), 1)
    # best mAh estimate: batterystats app + screen (model), else None
    if stats.get("app_mah") is not None:
        res["mah_est_app"] = stats["app_mah"]
        res["mah_est_app_plus_screen"] = round(stats["app_mah"] + (stats.get("screen_mah") or 0), 3)
        res["mah_per_hour_est"] = round(stats["app_mah"] * 3600.0 / elapsed, 1)
    hot = [s["hot"] for s in samples if s.get("hot") is not None]
    res["max_sensor_temp_c"] = max(hot) if hot else None
    return res


# ---------- main ----------
def main():
    global SERIAL
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--phase", default="all", choices=["scroll", "startup", "cpunet", "energy", "all"])
    ap.add_argument("--app", default="both", choices=["kotlin", "firn", "both"])
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--scroll-s", type=int, default=10)
    ap.add_argument("--energy-s", type=int, default=300)
    ap.add_argument("--serial", default=DEFAULT_SERIAL)
    ap.add_argument("--out", default=os.path.join(tempfile.gettempdir(), "benchapk", "results"))
    ap.add_argument("--no-lock", action="store_true", help="only for debugging; normally the lock is mandatory")
    a = ap.parse_args()
    SERIAL = a.serial
    os.makedirs(a.out, exist_ok=True)
    apps = ["kotlin", "firn"] if a.app == "both" else [a.app]
    jobs = []  # (phase, app)
    phases = ["scroll", "startup", "cpunet", "energy"] if a.phase == "all" else [a.phase]
    for ph in phases:
        if ph == "energy":
            jobs += [(ph, x) for x in apps]  # one app per slot
        else:
            jobs.append((ph, apps))
    for ph, target in jobs:
        if not a.no_lock:
            lock_acquire()
        try:
            prep_device()
            result = {"phase": ph, "date": datetime.datetime.now().isoformat(timespec="seconds"), "serial": SERIAL,
                      "model": sh("getprop ro.product.model").strip(), "android": sh("getprop ro.build.version.release").strip(),
                      "battery_start": battery_state(), "apps": {}}
            for x in (target if isinstance(target, list) else [target]):
                if ph == "scroll":
                    result["apps"][x] = phase_scroll(x, a.runs, a.scroll_s)
                elif ph == "startup":
                    result["apps"][x] = phase_startup(x, 5)
                elif ph == "cpunet":
                    result["apps"][x] = phase_cpunet(x, 3)
                else:
                    result["apps"][x] = phase_energy(x, a.energy_s, a.out)
            name = "%s_%s_%s.json" % (ph, a.app if ph != "energy" else x, time.strftime("%Y%m%d-%H%M%S"))
            with open(os.path.join(a.out, name), "w", encoding="utf-8") as f:
                json.dump(result, f, indent=1)
            print("RESULT_FILE " + os.path.join(a.out, name))
            for x, r in result["apps"].items():
                if ph == "energy":
                    print("SUMMARY %s %s %s" % (ph, x, json.dumps({k: r.get(k) for k in (
                        "elapsed_s", "plugged", "mah_est_app", "mah_est_app_plus_screen", "mah_per_hour_est", "app_cpu_ms",
                        "app_cpu_pct_of_one_core", "battery_temp_delta_c", "max_sensor_temp_c", "charge_counter_delta_mah")})))
                else:
                    print("SUMMARY %s %s %s" % (ph, x, json.dumps(r["median"])))
        finally:
            if not a.no_lock:
                lock_release()
        time.sleep(20)  # let other workers grab the phone between slots


if __name__ == "__main__":
    main()
