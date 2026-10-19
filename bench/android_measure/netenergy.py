#!/usr/bin/env python3
"""Network + energy comparison Firn vs Kotlin (uses measure.py helpers; run from the same folder).

  python netenergy.py --phase netb --netbase https://jarvis.fleitec.com/<dir>/ --runs 5 --out DIR
  python netenergy.py --phase energy --pairs 3 --energy-s 60 --out DIR

netb:   per run one launch of each app (interleaved, order alternates): ten fresh-connection GETs of nb_small.bin + one nb_big.bin
        (see examples/bench_android/netb.fi). Kotlin runs two passes (variant sys = default stack, cold = new SSLContext per request).
        Log lines `BENCH netb ...` are collected. Median over runs for every summary field.
energy: pairs of 60 s autoscroll runs (measure.phase_energy), order alternates, the phone cools down to baseline+0.3 C (max 300 s,
        min 45 s) before each run. Battery temperature before/after, app CPU time, batterystats estimate.
"""
import argparse, json, os, re, statistics, sys, time, datetime

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import measure as m


def netb_lines():
    return [l.strip() for l in m.bench_lines(r"netb") if l.strip().startswith("netb")]


def kv(line):
    return {k: v for k, v in re.findall(r"(\w+)=(-?\d+)", line)}


def one_netb(app, base, tmax=150):
    m.start(app, "--es tab lab --es lab net --ez autorun true --es theme dark --es netbase " + base)
    need = 2 if app == "kotlin" else 1
    t0 = time.time()
    while time.time() - t0 < tmax:
        time.sleep(2)
        m.lock_touch()
        if sum(1 for l in netb_lines() if l.startswith("netb_sum")) >= need:
            break
    ls = netb_lines()
    res = {}
    for l in ls:
        if l.startswith("netb_sum"):
            mv = re.search(r"variant=(\w+)", l)
            var = mv.group(1) if mv else "sys"
            res[var] = {k: int(v) for k, v in kv(l).items()}
    res["lines"] = ls
    m.sh("am force-stop " + m.APPS[app]["pkg"])
    return res


def phase_netb(apps, base, runs, out):
    runs_res = {a: [] for a in apps}
    for i in range(runs):
        order = apps if i % 2 == 0 else list(reversed(apps))
        for a in order:
            r = one_netb(a, base)
            runs_res[a].append(r)
            print("netb %s run %d: %s" % (a, i + 1, {k: v for k, v in r.items() if k != "lines"}), flush=True)
            time.sleep(3)
    summary = {}
    for a in apps:
        summary[a] = {}
        variants = set(k for r in runs_res[a] for k in r if k != "lines")
        for v in variants:
            keys = set(k for r in runs_res[a] for k in r.get(v, {}))
            summary[a][v] = {k: m.med([r.get(v, {}).get(k) for r in runs_res[a]]) for k in keys}
    return {"runs": runs_res, "median": summary}


def wait_cool(base_temp, tmin=45, tmax=300):
    t0 = time.time()
    while True:
        m.lock_touch()
        t = m.battery_state()["temp_c"]
        el = time.time() - t0
        if el >= tmin and (t is None or t <= base_temp + 0.3):
            return t, int(el)
        if el >= tmax:
            return t, int(el)
        time.sleep(10)


def phase_energy_pairs(apps, pairs, energy_s, out):
    base = m.battery_state()["temp_c"]
    res = {a: [] for a in apps}
    for i in range(pairs):
        order = apps if i % 2 == 0 else list(reversed(apps))
        for a in order:
            m.sh("am force-stop " + " ".join(x["pkg"] for x in m.APPS.values()))
            t_pre, waited = wait_cool(base)
            r = m.phase_energy(a, energy_s, out)
            r["temp_before_run_c"] = t_pre
            r["cooldown_s"] = waited
            res[a].append(r)
            keys = ("mah_est_app", "app_cpu_ms", "app_cpu_pct_of_one_core", "battery_temp_delta_c", "max_sensor_temp_c", "plugged")
            print("energy %s pair %d: pre=%s %s" % (a, i + 1, t_pre, json.dumps({k: r.get(k) for k in keys})), flush=True)
    med = {a: {k: m.med([r.get(k) for r in res[a]]) for k in (
        "mah_est_app", "mah_est_app_plus_screen", "mah_per_hour_est", "app_cpu_ms", "app_cpu_pct_of_one_core",
        "battery_temp_delta_c", "max_sensor_temp_c", "temp_before_run_c")} for a in apps}
    for a in apps:
        for r in res[a]:
            r["samples"] = [{k: s.get(k) for k in ("t_s", "temp_c", "hot", "cpu_ms")} for s in r.get("samples", [])]
    return {"base_temp_c": base, "runs": res, "median": med}


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--phase", required=True, choices=["netb", "energy"])
    ap.add_argument("--netbase", default="")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--pairs", type=int, default=3)
    ap.add_argument("--energy-s", type=int, default=60)
    ap.add_argument("--serial", default=m.DEFAULT_SERIAL)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    m.SERIAL = a.serial
    m.WHO = "fui-w-netz-energie"
    os.makedirs(a.out, exist_ok=True)
    apps = ["firn", "kotlin"]
    m.lock_acquire()
    try:
        m.prep_device()
        m.preflight(apps)
        meta = {"phase": a.phase, "date": datetime.datetime.now().isoformat(timespec="seconds"),
                "model": m.sh("getprop ro.product.model").strip(), "battery_start": m.battery_state()}
        if a.phase == "netb":
            meta.update(phase_netb(apps, a.netbase, a.runs, a.out))
        else:
            meta.update(phase_energy_pairs(apps, a.pairs, a.energy_s, a.out))
        name = "%s_%s.json" % (a.phase, time.strftime("%Y%m%d-%H%M%S"))
        with open(os.path.join(a.out, name), "w", encoding="utf-8") as f:
            json.dump(meta, f, indent=1)
        print("RESULT_FILE " + os.path.join(a.out, name))
        print("SUMMARY " + json.dumps(meta["median"]))
    finally:
        m.lock_release()


if __name__ == "__main__":
    main()
