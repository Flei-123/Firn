#!/usr/bin/env python3
"""All nine CPU kernels, Firn vs Kotlin, N runs each, median + checksum check + ratio.

  python cpubench.py [--runs 5] [--app both] [--out DIR] [--expected EXPECTED.txt]

Reuses measure.py (lock, start, logcat helpers). Per run: start the Lab/CPU tab with autorun,
wait for the `cpu` and `cpu2` BENCH lines, parse `name=ms:checksum`.
"""
import argparse, json, os, re, sys, tempfile, time, datetime

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import measure as M

KERNELS = ["primes", "sort", "mandel", "hash", "json", "strbuild", "matmul", "hashmap", "objsort"]
EXPECTED = {"primes": 110730338, "sort": 605536303, "mandel": 23077341, "hash": 829515813, "json": 866891154,
            "strbuild": 684235542, "matmul": 258052952, "hashmap": 49398548, "objsort": 717117234}


def one_run(app):
    M.lock_touch()
    M.start(app, "--es tab lab --es lab cpu --es theme dark --ez autorun true")
    l1 = M.wait_bench(r"\bcpu primes", 180)
    l2 = M.wait_bench(r"\bcpu2 ", 240)
    r = {}
    for line in (l1 or "", l2 or ""):
        for k, ms, cs in re.findall(r"(\w+)=(\d+):(\d+)", line):
            if k in KERNELS:
                r[k] = (int(ms), int(cs))
    return r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--app", default="both", choices=["firn", "kotlin", "both"])
    ap.add_argument("--out", default=os.path.join(tempfile.gettempdir(), "benchapk", "results"))
    a = ap.parse_args()
    apps = ["kotlin", "firn"] if a.app == "both" else [a.app]
    os.makedirs(a.out, exist_ok=True)
    M.lock_acquire()
    try:
        M.preflight(apps)
        M.prep_device()
        res = {x: [] for x in apps}
        # interleave runs so thermal drift hits both apps equally
        for i in range(a.runs):
            for app in apps:
                r = one_run(app)
                res[app].append(r)
                print("run %d %s: %s" % (i + 1, app, {k: v[0] for k, v in r.items()}), flush=True)
        for app in apps:
            M.sh("am force-stop " + M.APPS[app]["pkg"])
        med = {app: {k: M.med([r[k][0] for r in res[app] if k in r]) for k in KERNELS} for app in apps}
        bad = {app: [k for r in res[app] for k in r if r[k][1] != EXPECTED[k]] for app in apps}
        out = {"runs": {app: [{k: list(v) for k, v in r.items()} for r in res[app]] for app in apps},
               "median_ms": med, "checksum_errors": bad}
        if len(apps) == 2:
            out["ratio_firn_over_kotlin"] = {k: (round(med["firn"][k] / med["kotlin"][k], 2)
                                                 if med["firn"][k] and med["kotlin"][k] else None) for k in KERNELS}
        name = os.path.join(a.out, "cpubench_%s.json" % datetime.datetime.now().strftime("%H%M%S"))
        json.dump(out, open(name, "w"), indent=1)
        for app in apps:
            print("SUMMARY median_ms %s %s" % (app, med[app]))
            print("SUMMARY checksum_errors %s %s" % (app, bad[app]))
        if len(apps) == 2:
            print("SUMMARY ratio firn/kotlin %s" % out["ratio_firn_over_kotlin"])
        print("RESULT " + name)
    finally:
        M.lock_release()


if __name__ == "__main__":
    main()
