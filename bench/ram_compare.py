#!/usr/bin/env python3
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
FIRNC = os.environ.get("FIRNC_BIN", os.path.join(ROOT, "compiler/target/release/firnc"))
LEVEL = os.environ.get("OPT_LEVEL", "release-fast")
JOBS = int(os.environ.get("JOBS", "4"))
RUNS = int(os.environ.get("BENCH_RUNS", "3"))
ONLY = [x for x in os.environ.get("ONLY", "").split(",") if x]
OUT = os.environ.get("OUT", "")
VARIANTS = [v for v in os.environ.get("VARIANTS", "firn_off,firn_on,gcc,rust").split(",") if v]
TIMEOUT = int(os.environ.get("BENCH_TIMEOUT", "300"))

LOCAL = ["bitmap", "branchy", "bubblesort", "bytecount", "fib", "jsonscan", "matmul", "memstride",
         "sieve", "statemachine", "xxhash", "vec_sort", "sqrt", "vecsum", "vecstore", "vecsub", "vecforeign"]
VS_C = ["binary_trees", "bitops", "crc32", "fannkuch", "floatloop", "hashmap", "mandelbrot",
        "memcpy_loops", "nbody", "primes", "qsort", "sha256", "soa_access", "spectral_norm",
        "strbuild", "wordcount", "fnv_hash"]


def sources(name):
    if name in VS_C:
        firn = os.path.join(HERE, "firn_vs_c", "firn", name + ".fi")
        c = os.path.join(HERE, "firn_vs_c", "c", name + ".c")
    else:
        firn = os.path.join(HERE, "firn", name + ".fi")
        c = None
    rust = os.path.join(HERE, "rust", name + ".rs")
    return firn, (c if c and os.path.exists(c) else None), (rust if os.path.exists(rust) else None)


def build(variant, name, work):
    firn, c, rust = sources(name)
    out = os.path.join(work, name + "." + variant)
    if variant in ("firn_off", "firn_on"):
        mode = "on" if variant == "firn_on" else "off"
        cmd = [FIRNC, "--opt-level=" + LEVEL, "--ownership=" + mode, firn, "-o", out]
    elif variant == "gcc" and c:
        cmd = ["gcc", "-O2", "-fno-strict-aliasing", "-ffp-contract=off", c, "-o", out, "-lm"]
    elif variant == "gccs" and c:
        cmd = ["gcc", "-O2", "-static", "-fno-strict-aliasing", "-ffp-contract=off", c, "-o", out, "-lm"]
    elif variant == "rust" and rust:
        cmd = ["rustc", "-O", rust, "-o", out]
    else:
        return None
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        sys.stderr.write("build failed %s %s: %s\n" % (variant, name, (r.stderr or r.stdout)[-200:]))
        return None
    return out


def measure(binary):
    runs = []
    digest = None
    for _ in range(RUNS):
        r = subprocess.run(["/usr/bin/time", "-f", "TIMEFMT %M %U %S", binary], capture_output=True, timeout=TIMEOUT)
        err = r.stderr.decode("utf8", "replace").strip().splitlines()
        fields = [l for l in err if l.startswith("TIMEFMT ")][-1].split()[1:]
        digest = r.stdout[-60:].decode("utf8", "replace").strip().replace("\n", " ")
        runs.append((int(fields[0]), float(fields[1]) + float(fields[2]), r.returncode))
    return {
        "rss_kb": int(statistics.median(x[0] for x in runs)),
        "cpu_s": round(statistics.median(x[1] for x in runs), 3),
        "rc": runs[0][2],
        "size": os.path.getsize(binary),
        "out": digest,
    }


def job(args):
    name, work = args
    out = {}
    for variant in VARIANTS:
        b = build(variant, name, work)
        out[variant] = measure(b) if b else None
    return name, out


def fmt_ratio(a, b):
    return "%.2f" % (a / b) if a and b else "-"


def main():
    names = [n for n in LOCAL + VS_C if not ONLY or n in ONLY]
    work = tempfile.mkdtemp(prefix="ramcmp-")
    try:
        with ThreadPoolExecutor(JOBS) as ex:
            res = dict(ex.map(job, [(n, work) for n in names]))
    finally:
        shutil.rmtree(work, ignore_errors=True)
    rows = ["| bench | firn off KB | firn on KB | gcc KB | rust KB | on/gcc | on/rust | off cpu | on cpu | gcc cpu | rust cpu | size firn | size gcc | size rust | out same |",
            "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|"]
    ratios_gcc, ratios_rust = [], []
    for n in names:
        g = lambda v, k: (res[n].get(v) or {}).get(k)
        on, off, gc, ru = g("firn_on", "rss_kb"), g("firn_off", "rss_kb"), g("gcc", "rss_kb"), g("rust", "rss_kb")
        outs = set(g(v, "out") for v in VARIANTS if res[n].get(v))
        if on and gc:
            ratios_gcc.append(on / gc)
        if on and ru:
            ratios_rust.append(on / ru)
        rows.append("| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |" % (
            n, off or "-", on or "-", gc or "-", ru or "-", fmt_ratio(on, gc), fmt_ratio(on, ru),
            g("firn_off", "cpu_s") or "-", g("firn_on", "cpu_s") or "-", g("gcc", "cpu_s") or "-", g("rust", "cpu_s") or "-",
            g("firn_on", "size") or g("firn_off", "size") or "-", g("gcc", "size") or "-", g("rust", "size") or "-",
            "yes" if len(outs) <= 1 else "NO"))
    geo = lambda xs: statistics.geometric_mean(xs) if xs else 0
    rows.append("")
    rows.append("Geomean RSS firn on / gcc: %.2f (n=%d); firn on / rust: %.2f (n=%d)" % (geo(ratios_gcc), len(ratios_gcc), geo(ratios_rust), len(ratios_rust)))
    print("\n".join(rows))
    if OUT:
        with open(OUT, "w") as f:
            json.dump(res, f, indent=1)


if __name__ == "__main__":
    main()
