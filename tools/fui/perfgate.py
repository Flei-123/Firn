#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/fui/perfgate.py -- the performance gate of fUi.

    perfgate.py <perfbench-binary> [--update]

Runs the bench at 1240x720 and 1920x1080 (mode damage, 21 rounds each, best
of 3 runs per scenario), compares `norm` (median / reference loop, stable
under machine load) with tools/fui/perf_baseline.json and fails when a
scenario is more than 20 % worse (plus a small absolute floor for the
sub-millisecond scenarios). Also fails when a scenario misses its goal in
ms (goal_ms, measured on the unloaded reference machine via ref_ms).
--update rewrites the baseline from this run.

PINNING: a busy shared machine makes the 1920x1080 scenarios wobble even with
the reference-loop normalisation (a run on a loaded server once reported
scroll 2.79 against a baseline of 1.85, pinned on an idle core 1.57-1.67). The
gate therefore pins itself (and the bench it starts) to the idlest core and
its hyper-thread sibling, chosen from a 1 s sample of /proc/stat. Set
FUI_PERF_CPU=<n> to force a core, FUI_PERF_CPU=none to not pin.
"""
import json, os, re, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
BASE = os.path.join(HERE, "perf_baseline.json")
SIZES = [(1240, 720), (1920, 1080)]
TOL = 1.20
FLOOR = 0.03
RUNS = 5   # best of 5: a loaded shared server wobbles scroll/resize more than 20 % in 3 runs

def _busy():
    d = {}
    for line in open("/proc/stat"):
        if line.startswith("cpu") and line[3].isdigit():
            f = [int(x) for x in line.split()[1:]]
            d[int(line.split()[0][3:])] = (f[3] + f[4], sum(f))
    return d

def pin():
    want = os.environ.get("FUI_PERF_CPU", "")
    if want == "none" or not hasattr(os, "sched_setaffinity"):
        return None
    if want:
        cpus = {int(want)}
    else:
        a = _busy()
        time.sleep(1.0)
        b = _busy()
        use = {c: 1.0 - (b[c][0] - a[c][0]) / max(1, b[c][1] - a[c][1]) for c in a}
        best = None
        for c in a:
            try:
                sib = open("/sys/devices/system/cpu/cpu%d/topology/thread_siblings_list" % c).read()
                sib = {int(x) for x in sib.replace("-", ",").split(",")}
            except OSError:
                sib = {c}
            load = max(use.get(x, 1.0) for x in sib)
            if best is None or load < best[0]:
                best = (load, c)
        cpus = {best[1]}
    os.sched_setaffinity(0, cpus)
    return sorted(cpus)

def run(binary, w, h):
    best = {}
    for _ in range(RUNS):
        out = subprocess.run([binary, str(w), str(h), "21", "damage"],
                             capture_output=True, text=True, check=True).stdout
        for line in out.splitlines():
            if not line.startswith("PERF "):
                continue
            kv = dict(re.findall(r"(\w+)=(\S+)", line))
            name = kv["name"]
            norm = float(kv["norm"])
            if name not in best or norm < best[name]["norm"]:
                best[name] = {"norm": norm, "median_us": int(kv["median_us"]),
                              "p99_us": int(kv["p99_us"]),
                              "painted_px": int(kv["painted_px"]),
                              "ctrl_px": int(kv["ctrl_px"])}
    return best

def main():
    binary = sys.argv[1]
    update = "--update" in sys.argv
    print("pinned to cpu %s, load %s" % (pin(), open("/proc/loadavg").read().split()[0]))
    base = json.load(open(BASE)) if os.path.exists(BASE) else {"sizes": {}}
    bad = 0
    for (w, h) in SIZES:
        key = "%dx%d" % (w, h)
        got = run(binary, w, h)
        ref = base["sizes"].setdefault(key, {})
        for name, v in got.items():
            b = ref.get(name)
            # painted-pixel rules do not need a baseline (hoveranim has none)
            if not update:
                # hover/press must stay near the control area
                if name in ("hover", "press") and v["painted_px"] > 20 * v["ctrl_px"]:
                    print("  painted %d px for a %d px control: dirty rect too big" % (v["painted_px"], v["ctrl_px"]))
                    bad += 1
                # W6: a hover WITH its transition: every animation frame paints the two
                # buttons (leaving + entering), never the window
                if name == "hoveranim" and v["painted_px"] > 4 * v["ctrl_px"]:
                    print("  hoveranim painted %d px per frame for %d px of controls: transition frames repaint too much" % (v["painted_px"], v["ctrl_px"]))
                    bad += 1
                if name == "idle" and v["painted_px"] != 0:
                    print("  idle painted pixels"); bad += 1
            if update or b is None:
                keep = (b or {}).get("before_norm")
                ref[name] = {"norm": v["norm"], "median_us": v["median_us"],
                             "goal_ms": (b or {}).get("goal_ms", 16.0)}
                if keep is not None:
                    ref[name]["before_norm"] = keep
                continue
            limit = b["norm"] * TOL + FLOOR
            ok = v["norm"] <= limit
            print("%-8s %-7s norm %.3f (baseline %.3f, limit %.3f) median %d us  %s" %
                  (key, name, v["norm"], b["norm"], limit, v["median_us"], "ok" if ok else "REGRESSION"))
            if not ok:
                bad += 1
    if update:
        json.dump(base, open(BASE, "w"), indent=1, sort_keys=True)
        print("baseline written")
        return 0
    print("PERFGATE", "FAILED" if bad else "PASSED")
    return 1 if bad else 0

sys.exit(main())
