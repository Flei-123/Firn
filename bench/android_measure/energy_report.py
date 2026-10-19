#!/usr/bin/env python3
"""Table from the raw batterystats dumps + energy json of netenergy.py: python energy_report.py <outdir>
Per run: app mAh (model estimate, `UID u0aNNN: x` line of `dumpsys batterystats --usage`), screen mAh, app CPU ms, temperatures."""
import glob, json, os, re, statistics, subprocess, sys

out = sys.argv[1]
adb = os.path.join(os.path.expanduser("~"), "adbtools", "platform-tools", "adb.exe")


def pkg_uid(pkg):
    t = subprocess.run([adb, "shell", "pm list packages -U " + pkg], capture_output=True, text=True).stdout
    m = re.search(r"uid:(\d+)", t)
    return int(m.group(1)) if m else None


uids = {"firn": pkg_uid("com.fleitec.bench.firn"), "kotlin": pkg_uid("com.fleitec.bench.kotlin")}
print("uids", uids)
rows = {"firn": [], "kotlin": []}
for f in sorted(glob.glob(os.path.join(out, "batterystats_*.txt"))):
    app = os.path.basename(f).split("_")[1]
    txt = open(f, encoding="utf-8", errors="replace").read()
    u = uids[app]
    au = "u0a%d" % (u - 10000)
    m = re.search(r"UID %s:\s*([\d.]+)" % au, txt)
    mc = re.search(r"UID %s:.*\n.*cpu=([\d.]+) \(([^)]*)\)" % au, txt)
    ms = re.search(r"^\s*Screen:\s*([\d.]+)", txt, re.M | re.I) or re.search(r"screen:\s*([\d.]+)", txt, re.I)
    rows[app].append({"file": os.path.basename(f), "app_mah": float(m.group(1)) if m else None,
                      "cpu_mah": float(mc.group(1)) if mc else None, "cpu_time": mc.group(2) if mc else None,
                      "screen_mah": float(ms.group(1)) if ms else None})
for app, rs in rows.items():
    for r in rs:
        print(app, r)
    v = [r["app_mah"] for r in rs if r["app_mah"] is not None]
    if v:
        print("MEDIAN", app, "app_mah", round(statistics.median(v), 3), "n", len(v))
for jf in sorted(glob.glob(os.path.join(out, "energy_*.json"))):
    j = json.load(open(jf, encoding="utf-8"))
    for app in ("firn", "kotlin"):
        for i, r in enumerate(j["runs"][app]):
            print("RUN", app, i + 1, "pre_C", r.get("temp_before_run_c"), "cool_s", r.get("cooldown_s"), "cpu_ms", r.get("app_cpu_ms"),
                  "cpu_pct", r.get("app_cpu_pct_of_one_core"), "dT_batt", r.get("battery_temp_delta_c"), "max_sensor", r.get("max_sensor_temp_c"))
    print("MEDIAN_JSON", json.dumps(j["median"]))
