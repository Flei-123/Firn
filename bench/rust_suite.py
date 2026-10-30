#!/usr/bin/env python3
import hashlib
import json
import math
import os
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import threading
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
WORK = os.path.join(HERE, ".work")
FIRNC = os.environ.get("FIRNC_BIN", os.path.join(ROOT, "compiler/target/release/firnc"))
RUNS = int(os.environ.get("BENCH_RUNS", "5"))
JOBS = int(os.environ.get("JOBS", "4"))
MODE = os.environ.get("MODE", "both")
ONLY = [x for x in os.environ.get("ONLY", "").split(",") if x]
OUT = os.environ.get("OUT", os.path.join(WORK, "rust_suite.json"))
TIMEOUT = int(os.environ.get("BENCH_TIMEOUT", "900"))
NO_SAFE = os.environ.get("NO_SAFE") == "1"
LIMIT = float(os.environ.get("FACTOR_LIMIT", "1.2"))
RUSTFLAGS = os.environ.get("RUST_SUITE_FLAGS", "-O").split()
CACHE = os.path.join(WORK, "rust_cache.json")

LOCAL = ["bitmap", "branchy", "bubblesort", "bytecount", "fib", "jsonscan", "matmul", "memstride",
         "sieve", "statemachine", "xxhash", "vec_sort", "sqrt", "vecsum", "vecstore", "vecsub", "vecforeign"]
VS_C = ["binary_trees", "bitops", "crc32", "fannkuch", "floatloop", "hashmap", "mandelbrot",
        "memcpy_loops", "nbody", "primes", "qsort", "sha256", "soa_access", "spectral_norm",
        "strbuild", "wordcount", "fnv_hash"]
RENAMED = {"fib40": "fib", "matmul700": "matmul", "jsonscan28": "jsonscan"}


def pairs():
    out = []
    for n in LOCAL:
        out.append((n, os.path.join(HERE, "firn", n + ".fi"), os.path.join(HERE, "rust", n + ".rs")))
    for n in VS_C:
        out.append((n, os.path.join(HERE, "firn_vs_c", "firn", n + ".fi"), os.path.join(HERE, "rust", n + ".rs")))
    for n, src in RENAMED.items():
        out.append((n, os.path.join(HERE, "firn_vs_c", "firn", src + ".fi"), os.path.join(HERE, "rust", n + ".rs")))
    return [p for p in out if not ONLY or p[0] in ONLY]


def file_hash(*paths):
    h = hashlib.sha256()
    for p in paths:
        with open(p, "rb") as f:
            h.update(f.read())
    return h.hexdigest()[:16]


def run(cmd, timeout=None):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


def build_firn(src, out, level):
    r = run([FIRNC, "--opt-level=" + level, src, "-o", out])
    return r.returncode == 0, (r.stderr or r.stdout)[-300:]


def build_rust(src, out):
    r = run(["rustc"] + RUSTFLAGS + [src, "-o", out])
    return r.returncode == 0, (r.stderr or r.stdout)[-300:]


def count_instr(binary, tag, tmp):
    cg = os.path.join(tmp, tag + ".cg")
    try:
        r = run(["valgrind", "--tool=callgrind", "--callgrind-out-file=" + cg, binary], timeout=TIMEOUT)
    except subprocess.TimeoutExpired:
        return None, "timeout"
    if os.path.exists(cg):
        os.remove(cg)
    m = re.search(r"Collected\s*:\s*(\d+)", r.stderr)
    if not m:
        return None, "no count"
    return int(m.group(1)), r.stdout.strip() + "|exit=%d" % r.returncode


def run_cpu(binary):
    p = subprocess.Popen([binary], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    timer = threading.Timer(TIMEOUT, p.kill)
    timer.start()
    out = p.stdout.read()
    _, status, ru = os.wait4(p.pid, 0)
    timer.cancel()
    if os.WIFSIGNALED(status):
        return None, "killed"
    code = os.WEXITSTATUS(status)
    return ru.ru_utime + ru.ru_stime, out.decode().strip() + "|exit=%d" % code


def ratio(a, b):
    if a is None or not b:
        return None
    return a / b


def geomean(xs):
    xs = [x for x in xs if x]
    if not xs:
        return None
    return math.exp(sum(math.log(x) for x in xs) / len(xs))


def one(spec, cache, tmp):
    name, fsrc, rsrc = spec
    e = {"firn_src": os.path.relpath(fsrc, ROOT), "rust_src": os.path.relpath(rsrc, ROOT), "err": []}
    bins = {}
    for key, level in (("fast", "release-fast"), ("safe", "release-safe")):
        if key == "safe" and NO_SAFE:
            continue
        out = os.path.join(tmp, name + "." + key)
        ok, msg = build_firn(fsrc, out, level)
        if ok:
            bins[key] = out
        else:
            e["err"].append("firn %s build: %s" % (key, msg))
    rbin = os.path.join(tmp, name + ".rust")
    ok, msg = build_rust(rsrc, rbin)
    if ok:
        bins["rust"] = rbin
    else:
        e["err"].append("rust build: " + msg)
    outputs = {}
    if MODE in ("instr", "both"):
        rkey = file_hash(rsrc) + ":" + " ".join(RUSTFLAGS)
        cached = cache.get(name)
        for k, b in bins.items():
            if k == "rust" and cached and cached.get("key") == rkey and not os.environ.get("NO_CACHE"):
                e["rust_instr"] = cached["instr"]
                outputs["rust"] = cached["out"]
                continue
            n, o = count_instr(b, name + "." + k, tmp)
            if n is None:
                e["err"].append("%s instr: %s" % (k, o))
                continue
            e[k + "_instr"] = n
            outputs[k] = o
            if k == "rust":
                cache[name] = {"key": rkey, "instr": n, "out": o}
    if MODE in ("wall", "both"):
        samples = {k: [] for k in bins}
        for r in range(RUNS + 1):
            for k, b in bins.items():
                cpu, o = run_cpu(b)
                if cpu is None:
                    e["err"].append("%s wall: %s" % (k, o))
                    continue
                outputs.setdefault(k, o)
                if r > 0:
                    samples[k].append(cpu)
        for k, s in samples.items():
            if s:
                e[k + "_wall"] = statistics.median(s)
    ref = outputs.get("rust")
    e["output"] = ref
    ok = {k: outputs.get(k) == ref for k in bins if k != "rust"}
    for k, good in ok.items():
        if not good:
            e["err"].append("MISMATCH %s: %r vs rust %r" % (k, outputs.get(k), ref))
    e["match"] = all(ok.values()) and ref is not None
    if ok.get("fast"):
        e["ratio_fast"] = ratio(e.get("fast_instr"), e.get("rust_instr"))
        e["wall_fast"] = ratio(e.get("fast_wall"), e.get("rust_wall"))
    if ok.get("safe"):
        e["ratio_safe"] = ratio(e.get("safe_instr"), e.get("rust_instr"))
        e["wall_safe"] = ratio(e.get("safe_wall"), e.get("rust_wall"))
    return name, e


def fmt(v, spec="%.3f"):
    return "-" if v is None else spec % v


def summary(res):
    s = {}
    for key in ("ratio_fast", "ratio_safe", "wall_fast", "wall_safe"):
        xs = [(n, e[key]) for n, e in res["pairs"].items() if e.get(key)]
        if not xs:
            continue
        vals = [v for _, v in xs]
        mx = max(xs, key=lambda t: t[1])
        s[key] = {
            "count": len(vals),
            "median": statistics.median(vals),
            "geomean": geomean(vals),
            "max": mx[1],
            "max_pair": mx[0],
            "over_limit": sorted([(n, v) for n, v in xs if v > LIMIT], key=lambda t: -t[1]),
        }
    res["summary"] = s


def print_table(res):
    print("%-14s %14s %14s %14s %8s %8s %8s %8s" % ("pair", "firn_fast", "firn_safe", "rust", "fast/r", "safe/r", "wallF/r", "wallS/r"))
    for n, e in res["pairs"].items():
        flag = "" if e["match"] and not e["err"] else "  !! " + "; ".join(e["err"])[:120]
        print("%-14s %14s %14s %14s %8s %8s %8s %8s%s" % (
            n, fmt(e.get("fast_instr"), "%d"), fmt(e.get("safe_instr"), "%d"), fmt(e.get("rust_instr"), "%d"),
            fmt(e.get("ratio_fast")), fmt(e.get("ratio_safe")), fmt(e.get("wall_fast")), fmt(e.get("wall_safe")), flag))
    print()
    print("pairs: %d  load: %s  rustc: %s" % (len(res["pairs"]), " ".join(res["load"]), res["rustc"]))
    for key, label in (("ratio_fast", "instr release-fast / rustc -O"), ("ratio_safe", "instr release-safe / rustc -O"),
                       ("wall_fast", "wall release-fast / rustc -O"), ("wall_safe", "wall release-safe / rustc -O")):
        s = res["summary"].get(key)
        if not s:
            continue
        print("%-34s n=%d median=%.3f geomean=%.3f max=%.3f (%s)" % (label, s["count"], s["median"], s["geomean"], s["max"], s["max_pair"]))
        if s["over_limit"]:
            print("  > %.2fx: %s" % (LIMIT, ", ".join("%s %.2f" % t for t in s["over_limit"])))


def main():
    os.makedirs(WORK, exist_ok=True)
    cache = {}
    if os.path.exists(CACHE):
        try:
            cache = json.load(open(CACHE))
        except ValueError:
            cache = {}
    specs = pairs()
    tmp = tempfile.mkdtemp(prefix="rust_suite.")
    commit = run(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"]).stdout.strip()
    res = {
        "commit": commit,
        "firnc": FIRNC,
        "rustc": run(["rustc", "--version"]).stdout.strip(),
        "rustflags": RUSTFLAGS,
        "mode": MODE,
        "runs": RUNS,
        "load": open("/proc/loadavg").read().split()[:3],
        "pairs": {},
    }
    try:
        with ThreadPoolExecutor(max_workers=JOBS) as ex:
            for n, e in ex.map(lambda s: one(s, cache, tmp), specs):
                res["pairs"][n] = e
                print(n, e.get("ratio_fast"), e["err"] or "", flush=True)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    res["load_end"] = open("/proc/loadavg").read().split()[:3]
    summary(res)
    with open(OUT, "w") as f:
        json.dump(res, f, indent=1)
    with open(CACHE, "w") as f:
        json.dump(cache, f, indent=1)
    print_table(res)


if __name__ == "__main__":
    main()
