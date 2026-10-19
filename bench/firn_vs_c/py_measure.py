#!/usr/bin/env python3
"""Python side of the Firn-vs-C suite: time + checksum + size for py/ and pure/.

For every task:
  * builds c/<name>.c with gcc -O2 (temp dir), runs it -> reference checksum + C time
  * runs py/<name>.py (idiomatic) and pure/<name>.py (pure loops), median of up to 3 runs,
    CPU time = user+sys of exactly that child (os.wait4 rusage; same counters as
    RUSAGE_CHILDREN but safe with parallel jobs)
  * checksum must equal the C output, otherwise the row is MISMATCH and gets no time
  * counts code lines (non-empty, non-comment) and tokens of .fi / .c / py / pure

Long pure runs: SCALE_PURE / SCALE_IDIOM hold a divisor S (env PYN_SCALE). The scaled
program is verified against a gcc build of the SAME scaled C source (constants patched),
and the time is multiplied by the work ratio (EXTRAP) and flagged "extrapolated".
Nothing is extrapolated unless the task is listed in the scale tables.

  python3 bench/firn_vs_c/py_measure.py [--only a,b] [--jobs 6] [--runs 3] [--no-pure]
Writes py_results.json and py_results.md next to this file.
"""
import argparse
import io
import json
import os
import re
import resource  # noqa: F401  (documented counter source)
import statistics
import subprocess
import sys
import tempfile
import tokenize
from concurrent.futures import ThreadPoolExecutor

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
FIRNC = os.environ.get("FIRNC_BIN", os.path.join(ROOT, "compiler/target/release/firnc"))
TIMEOUT = int(os.environ.get("PY_TIMEOUT", "300"))

TASKS = ["primes", "qsort", "mandelbrot", "nbody", "fannkuch", "spectral_norm", "matmul", "fib",
         "bitops", "floatloop", "fnv_hash", "sha256", "jsonscan", "strbuild", "memcpy_loops",
         "binary_trees", "hashmap", "soa_access", "crc32", "wordcount"]

# divisor S (PYN_SCALE) for the runs; absent = full size. fib / fannkuch use a parameter instead.
SCALE_PURE = {"nbody": 4, "bitops": 10, "floatloop": 2, "fnv_hash": 4, "crc32": 4, "sha256": 2,
              "jsonscan": 4}
SCALE_IDIOM = {"nbody": 2, "bitops": 4, "floatloop": 2, "fnv_hash": 2}
# fib: PYN_FIBN=n; fannkuch: PYN_N=n   (pure only)
PARAM_PURE = {"fib": 35}

# C source patches for a divisor S: list of (regex, template with {v}, base value)
CPATCH = {
    "primes": [(r"(#define LIMIT )20000000UL", "{v}UL", 20000000)],
    "qsort": [(r"(#define N )2000000", "{v}", 2000000)],
    "mandelbrot": [(r"(#define SIZE )800", "{v}", 800)],
    "nbody": [(r"(#define STEPS )4000000", "{v}", 4000000)],
    "fnv_hash": [(r"(size_t n = )67108864", "{v}", 67108864)],
    "crc32": [(r"(size_t n = )67108864", "{v}", 67108864)],
    "sha256": [(r"(size_t n = )16777216", "{v}", 16777216)],
    "bitops": [(r"(#define STEPS )200000000ULL", "{v}ULL", 200000000)],
    "floatloop": [(r"(#define STEPS )100000000LL", "{v}LL", 100000000)],
    "matmul": [(r"(#define N )700", "{v}", 700)],
    "spectral_norm": [(r"(#define N )1500", "{v}", 1500)],
    "memcpy_loops": [(r"(r < )12", "{v}", 12)],
    "strbuild": [(r"(i < )1000000", "{v}", 1000000)],
    "jsonscan": [(r"(u64 count = )350000", "{v}", 350000)],
    "wordcount": [(r"(size_t nwords = )3000000", "{v}", 3000000)],
}
# work ratio full/scaled, default = S (linear)
EXTRAP = {
    "matmul": lambda S: float(S) ** 3,
    "spectral_norm": lambda S: float(S) ** 2,
    "mandelbrot": lambda S: float(S) ** 2,
}
NOTES = {
    "matmul": "work ~ N^3 (ratio of N^3)",
    "spectral_norm": "work ~ N^2 (ratio of N^2)",
    "mandelbrot": "work ~ pixels (approx, different sample grid)",
    "memcpy_loops": "per-round work; final 2M-word loop (<1%) counted S times",
    "fib": "exact call-count ratio 2*F(n+1)-1",
}


def fibn_calls(n):
    a, b = 0, 1
    for _ in range(n + 1):
        a, b = b, a + b
    return 2 * a - 1


def run_child(argv, env_extra=None, cwd=None):
    """Run argv, return (cpu_seconds user+sys, stdout) or (None, reason)."""
    env = dict(os.environ)
    env.update({"OPENBLAS_NUM_THREADS": "1", "OMP_NUM_THREADS": "1", "MKL_NUM_THREADS": "1"})
    env.update(env_extra or {})
    p = subprocess.Popen(argv, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, cwd=cwd)
    import threading
    out_box = {}

    def reader():
        out_box["out"], out_box["err"] = p.stdout.read(), p.stderr.read()

    th = threading.Thread(target=reader)
    th.start()
    done = threading.Event()
    res = {}

    def waiter():
        _, status, ru = os.wait4(p.pid, 0)
        res["status"], res["ru"] = status, ru
        done.set()

    tw = threading.Thread(target=waiter)
    tw.start()
    if not done.wait(TIMEOUT):
        p.kill()
        done.wait()
        th.join()
        tw.join()
        return None, "TIMEOUT"
    th.join()
    tw.join()
    ru = res["ru"]
    if res["status"] != 0:
        return None, "exit %s: %s" % (res["status"], out_box["err"][-200:].decode("utf8", "replace"))
    return ru.ru_utime + ru.ru_stime, out_box["out"].decode().strip()


def median_runs(argv, env, runs):
    times, out = [], None
    for i in range(runs):
        t, o = run_child(argv, env)
        if t is None:
            return None, o
        times.append(t)
        out = o
        if i == 0 and t > 25:       # long run: one sample is enough, say so
            return t, o
    return statistics.median(times), out


def build_c(src_text, d, name):
    src = os.path.join(d, name + ".c")
    exe = os.path.join(d, name)
    with open(src, "w") as f:
        f.write(src_text)
    r = subprocess.run(["gcc", "-O2", "-fno-strict-aliasing", src, "-o", exe, "-lm"],
                       capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError("gcc failed: " + r.stderr[-300:])
    return exe


def patch_c(text, name, S):
    for rx, tmpl, base in CPATCH[name]:
        new, n = re.subn(rx, lambda m: m.group(1) + tmpl.format(v=base // S), text, count=1)
        if n != 1:
            raise RuntimeError("C patch failed for %s: %s" % (name, rx))
        text = new
    return text


def count_lines_tokens(path, kind):
    """(code lines, tokens); lines = non-empty non-comment."""
    if not os.path.exists(path):
        return None, None
    text = open(path, encoding="utf8").read()
    if kind == "py":
        lines = 0
        for ln in text.splitlines():
            s = ln.strip()
            if s and not s.startswith("#"):
                lines += 1
        skip = {tokenize.COMMENT, tokenize.NL, tokenize.NEWLINE, tokenize.INDENT, tokenize.DEDENT,
                tokenize.ENDMARKER, tokenize.ENCODING}
        toks = sum(1 for t in tokenize.generate_tokens(io.StringIO(text).readline) if t.type not in skip)
        return lines, toks
    # c and fi: strip comments
    t = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    t = re.sub(r"//[^\n]*", "", t)
    lines = sum(1 for ln in t.splitlines() if ln.strip())
    if kind == "fi":
        r = subprocess.run([FIRNC, "--emit=tokens", path], capture_output=True, text=True)
        n = [ln for ln in r.stdout.splitlines() if ln.strip()]
        n = [ln for ln in n if not ln.rstrip().endswith("Eof")]
        return lines, len(n)
    strip_str = re.sub(r'"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])*\'', "S", t)
    toks = re.findall(r"[A-Za-z_]\w*|\d[\w.]*|S|->|<<=|>>=|\+\+|--|<<|>>|<=|>=|==|!=|&&|\|\||[-+*/%&|^]=|\S",
                      strip_str)
    return lines, len(toks)


def measure_variant(name, kind, c_cache, runs, scale, param, tmpd):
    """kind 'py' or 'pure'. Returns result dict."""
    path = os.path.join(HERE, kind, name + ".py")
    if not os.path.exists(path):
        return {"status": "missing"}
    env = {}
    ref = c_cache["ref"]
    note = None
    factor = 1.0
    if param is not None:                       # fib / fannkuch: parameter override
        env["PYN_FIBN" if name == "fib" else "PYN_N"] = str(param)
        if name == "fib":
            factor = fibn_calls(40) / fibn_calls(param)
            patched = c_cache["src"].replace("fib(40)", "fib(%d)" % param)
        else:
            patched = c_cache["src"].replace("#define N 10", "#define N %d" % param)
        exe = build_c(patched, tmpd, name + "_p%d" % param)
        _, ref = run_child([exe])
        note = "%s=%d, extrapolated x%.1f (%s)" % ("n" if name == "fib" else "n", param, factor,
                                                    NOTES.get(name, "work ratio"))
    elif scale and scale > 1:
        env["PYN_SCALE"] = str(scale)
        exe = build_c(patch_c(c_cache["src"], name, scale), tmpd, name + "_s%d" % scale)
        _, ref = run_child([exe])
        factor = EXTRAP.get(name, lambda S: float(S))(scale)
        note = "scale 1/%d, extrapolated x%.1f%s" % (scale, factor, "; " + NOTES[name] if name in NOTES else "")
    t, out = median_runs([sys.executable, "-I", path], env, runs)
    if t is None:
        return {"status": out if out == "TIMEOUT" else "ERROR", "detail": out}
    if out != ref:
        return {"status": "MISMATCH", "out": out, "ref": ref, "cpu_raw": round(t, 3), "note": note}
    res = {"status": "ok", "cpu_s": round(t * factor, 3), "cpu_raw_s": round(t, 3), "output": out}
    if note:
        res["extrapolated"] = True
        res["note"] = note
        res["factor"] = round(factor, 2)
    return res


def do_task(name, args):
    r = {"task": name}
    with tempfile.TemporaryDirectory(prefix="pym-") as tmpd:
        src = open(os.path.join(HERE, "c", name + ".c")).read()
        exe = build_c(src, tmpd, name)
        t, out = median_runs([exe], {}, 3)
        r["c"] = {"cpu_s": round(t, 3), "output": out}
        cc = {"src": src, "ref": out}
        r["idiomatic"] = measure_variant(name, "py", cc, args.runs, SCALE_IDIOM.get(name), None, tmpd)
        if not args.no_pure:
            r["pure"] = measure_variant(name, "pure", cc, args.runs, SCALE_PURE.get(name),
                                        PARAM_PURE.get(name), tmpd)
    sizes = {}
    for key, path, kind in (("fi", "firn/%s.fi" % name, "fi"), ("c", "c/%s.c" % name, "c"),
                            ("py", "py/%s.py" % name, "py"), ("pure", "pure/%s.py" % name, "py")):
        lines, toks = count_lines_tokens(os.path.join(HERE, path), kind)
        sizes[key] = {"lines": lines, "tokens": toks}
    r["size"] = sizes
    print("done", name, file=sys.stderr, flush=True)
    return r


def fmt_t(v):
    if v is None:
        return "-"
    if v.get("status") != "ok":
        return v.get("status", "?")
    s = "%.2f" % v["cpu_s"]
    return s + "*" if v.get("extrapolated") else s


def write_md(results, path):
    L = ["# Firn vs C vs Python: Python side", "",
         "CPU seconds (user+sys), median of up to 3 runs (runs > 25 s: single sample); `*` = extrapolated from a scaled run "
         "(see notes). Checksums verified against the C output. Lines = non-empty non-comment; "
         "tokens = Python `tokenize` / `firnc --emit=tokens` (without Eof) / regex tokenizer for C.", "",
         "| task | C -O2 s | py (idiomatic) s | py x C | pure py s | pure x C | "
         "fi lines/tok | c | py | pure |", "|---|---|---|---|---|---|---|---|---|---|"]

    def ratio(v, c):
        if v and v.get("status") == "ok" and c:
            return "%.0f" % (v["cpu_s"] / c) if v["cpu_s"] / c >= 10 else "%.1f" % (v["cpu_s"] / c)
        return "-"

    def sz(r, k):
        s = r["size"][k]
        return "-" if s["lines"] is None else "%d/%d" % (s["lines"], s["tokens"])

    for r in results:
        c = r["c"]["cpu_s"]
        L.append("| %s | %.2f | %s | %s | %s | %s | %s | %s | %s | %s |" % (
            r["task"], c, fmt_t(r.get("idiomatic")), ratio(r.get("idiomatic"), c), fmt_t(r.get("pure")),
            ratio(r.get("pure"), c), sz(r, "fi"), sz(r, "c"), sz(r, "py"), sz(r, "pure")))
    L += ["", "Size columns: code lines / tokens.", "", "## Notes", ""]
    for r in results:
        for k in ("idiomatic", "pure"):
            v = r.get(k) or {}
            if v.get("note"):
                L.append("- %s %s: %s" % (r["task"], k, v["note"]))
            if v.get("status") not in (None, "ok"):
                L.append("- %s %s: %s %s" % (r["task"], k, v.get("status"), v.get("detail", v.get("out", ""))))
    open(path, "w").write("\n".join(L) + "\n")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--only", default="")
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--runs", type=int, default=3)
    ap.add_argument("--no-pure", action="store_true")
    ap.add_argument("--out", default=os.path.join(HERE, "py_results"))
    args = ap.parse_args()
    names = [t for t in (args.only.split(",") if args.only else TASKS) if t]
    with ThreadPoolExecutor(max_workers=args.jobs) as ex:
        results = list(ex.map(lambda n: do_task(n, args), names))
    results.sort(key=lambda r: TASKS.index(r["task"]))
    json.dump({"scale_pure": SCALE_PURE, "scale_idiomatic": SCALE_IDIOM, "param_pure": PARAM_PURE,
               "results": results}, open(args.out + ".json", "w"), indent=1)
    write_md(results, args.out + ".md")
    bad = [r["task"] for r in results
           if any((r.get(k) or {}).get("status") not in (None, "ok") for k in ("idiomatic", "pure"))]
    print("problems:", bad or "none")


if __name__ == "__main__":
    main()
