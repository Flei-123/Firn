#!/usr/bin/env python3
"""tools/stats_cross/cross.py -- cross-check of lib/std/stats.fi against Python.

    cross.py gen   <ndatasets> <seed> <in.bin> <classes.txt>
    cross.py check <in.bin> <out.bin> <classes.txt> [--strict]

`gen` writes random data sets (several families: uniform, decimals, wide and
tiny magnitudes, ill-conditioned, ties, constants, sorted, byte values,
cancellation, 1 and 2 elements) in the binary format of stats_cli.fi.
`check` reads the answers of stats_cli and compares them with

    sum          math.fsum                      (exactly rounded)
    mean         statistics.fmean, statistics.mean (exact fractions, n <= 400)
    median       statistics.median, numpy.median
    percentile   numpy.percentile (default 'linear'); statistics.quantiles
                 (method="inclusive") at p = 10, 20 .. 90
    variance     statistics.variance (exact fractions), numpy.var(ddof=1)
    pvariance    statistics.pvariance
    stdev        statistics.stdev / pstdev
    histogram    numpy.histogram(bins=16, range=(lo, hi))

and reports the largest deviation in ULP (units in the last place) per
statistic and per family. Nothing is hidden: with --strict a deviation above
the documented tolerance (TOL below) is an error, otherwise it is only
reported. numpy is optional (its rows are skipped when it is missing).
"""
import math
import random
import statistics
import struct
import sys

try:
    import numpy as np
except ImportError:  # numpy is optional
    np = None

PCTS = [0.0, 1.0, 5.0, 10.0, 20.0, 25.0, 30.0, 33.3, 40.0, 50.0, 60.0, 66.6,
        70.0, 75.0, 80.0, 90.0, 95.0, 99.0, 99.9, 100.0]
OUTW = 49

# Documented tolerances in ULP (see docs/SKRIPT-LIBS.md, "std.stats"): the
# measured maxima over 5200 data sets are sum/mean 0-1, variance family 3.
TOL = {
    "sum_exact": 0, "mean_exact": 0, "sum": 2, "mean": 2, "min": 0, "max": 0,
    "median": 0, "percentile": 0, "variance": 4, "pvariance": 4, "stdev": 4,
    "pstdev": 4,
}
TOL_FAMILY = {
    # Neumaier has no bound when terms of wildly different magnitude cancel
    # (1e30 against 1; see stats.fi H3): reported, not judged. sum_exact and
    # mean_exact are the exactly rounded forms and are judged at 0 ulp.
    "cancel": {"sum": None, "mean": None},
}


def f2b(x):
    return struct.unpack("<Q", struct.pack("<d", x))[0]


def b2f(b):
    return struct.unpack("<d", struct.pack("<Q", b))[0]


def ordered(x):
    i = struct.unpack("<q", struct.pack("<d", x))[0]
    return i if i >= 0 else -(i & 0x7FFFFFFFFFFFFFFF)


def ulp(a, b):
    """Distance in ULP; None when one is NaN and the other is not (a real error)."""
    if math.isnan(a) or math.isnan(b):
        return 0 if (math.isnan(a) and math.isnan(b)) else None
    if a == b:
        return 0
    return abs(ordered(a) - ordered(b))


# ------------------------------------------------------------------ gen
def families(rng):
    def uniform():
        return [rng.uniform(-1, 1) for _ in range(rng.randint(1, 300))]

    def decimals():
        return [round(rng.uniform(0, 100), 2) for _ in range(rng.randint(1, 300))]

    def wide():
        return [rng.choice((-1, 1)) * 10 ** rng.uniform(-100, 100)
                for _ in range(rng.randint(1, 200))]

    def tiny():
        return [rng.choice((-1, 1)) * 10 ** rng.uniform(-140, -100)
                for _ in range(rng.randint(1, 100))]

    def denormal():
        return [rng.choice((-1, 1)) * rng.randint(1, 2 ** 40) * 5e-324
                for _ in range(rng.randint(1, 60))]

    def huge():
        return [rng.choice((-1, 1)) * 10 ** rng.uniform(100, 150)
                for _ in range(rng.randint(1, 100))]

    def illcond():
        mag = 10 ** rng.uniform(0, 9)
        noise = 10 ** -rng.uniform(0, 3)
        return [mag + rng.uniform(-1, 1) * noise for _ in range(rng.randint(2, 300))]

    def ties():
        k = rng.randint(1, 5)
        vals = [round(rng.uniform(-50, 50), 1) for _ in range(k)]
        return [rng.choice(vals) for _ in range(rng.randint(1, 400))]

    def tiny_sets():
        n = rng.choice((1, 2, 2, 3))
        return [rng.uniform(-1e3, 1e3) for _ in range(n)]

    def constant():
        return [rng.uniform(-1e3, 1e3)] * rng.randint(1, 200)

    def sorted_asc():
        v = sorted(rng.gauss(0, 10) for _ in range(rng.randint(2, 300)))
        return v

    def sorted_desc():
        return sorted((rng.gauss(0, 10) for _ in range(rng.randint(2, 300))), reverse=True)

    def gauss():
        mu = rng.choice((0, 1e6, -1e3, 12.5))
        return [rng.gauss(mu, rng.choice((1, 1e-3, 1e3))) for _ in range(rng.randint(2, 500))]

    def bytes_():
        return [float(rng.randint(0, 255)) for _ in range(rng.choice((50, 500, 5000, 20000)))]

    def cancel():
        base = [rng.uniform(1, 2) * 10 ** rng.uniform(0, 30) for _ in range(rng.randint(1, 60))]
        v = base + [-x for x in base] + [rng.uniform(-1, 1) for _ in range(rng.randint(1, 20))]
        rng.shuffle(v)
        return v

    def mixed_signs():
        return [rng.gauss(0, 1) * 10 ** rng.randint(-8, 8) for _ in range(rng.randint(2, 200))]

    return [("uniform", uniform, 14), ("decimals", decimals, 10), ("wide", wide, 8),
            ("tiny", tiny, 4), ("denormal", denormal, 3), ("huge", huge, 4), ("illcond", illcond, 8),
            ("ties", ties, 10), ("small", tiny_sets, 8), ("constant", constant, 5),
            ("sorted", sorted_asc, 4), ("sorted", sorted_desc, 4), ("gauss", gauss, 10),
            ("bytes", bytes_, 3), ("cancel", cancel, 6), ("mixed", mixed_signs, 8)]


def gen(nsets, seed, path, cpath):
    rng = random.Random(seed)
    fams = families(rng)
    weights = [w for (_, _, w) in fams]
    words = [nsets]
    classes = []
    for _ in range(nsets):
        name, fn, _w = rng.choices(fams, weights)[0]
        data = fn()
        lo, hi = min(data), max(data)
        words += [len(data), f2b(lo), f2b(hi)] + [f2b(x) for x in data]
        classes.append(name)
    with open(path, "wb") as f:
        f.write(struct.pack("<%dQ" % len(words), *words))
    with open(cpath, "w") as f:
        f.write("\n".join(classes) + "\n")


# ---------------------------------------------------------------- check
def read_sets(path):
    raw = open(path, "rb").read()
    w = struct.unpack("<%dQ" % (len(raw) // 8), raw)
    nsets, pos, sets = w[0], 1, []
    for _ in range(nsets):
        n = w[pos]
        lo, hi = b2f(w[pos + 1]), b2f(w[pos + 2])
        sets.append((lo, hi, [b2f(x) for x in w[pos + 3: pos + 3 + n]]))
        pos += 3 + n
    return sets


class Stat:
    def __init__(self):
        self.max_ulp = {}      # (stat, family) -> max ulp
        self.over = {}         # (stat, family) -> count above tolerance
        self.n = {}            # (stat, family) -> cases
        self.nan_bad = 0
        self.worst = {}        # stat -> (ulp, family, dataset index)

    def add(self, stat, fam, a, b, idx, ref_ok=True):
        if not ref_ok:
            return
        u = ulp(a, b)
        key = (stat, fam)
        self.n[key] = self.n.get(key, 0) + 1
        if u is None:
            self.nan_bad += 1
            self.over[key] = self.over.get(key, 0) + 1
            print("  NaN mismatch: %s family=%s #%d ours=%r ref=%r" % (stat, fam, idx, a, b))
            return
        if u > self.max_ulp.get(key, -1):
            self.max_ulp[key] = u
        base = stat.split("/")[0]
        tol = TOL_FAMILY.get(fam, {}).get(base, TOL.get(base, 0))
        if tol is not None and u > tol:
            self.over[key] = self.over.get(key, 0) + 1
        w = self.worst.get(stat)
        if w is None or u > w[0]:
            self.worst[stat] = (u, fam, idx)


EPS = 2.220446049250313e-16


class QuantileStat:
    """statistics.quantiles(method="inclusive") evaluates the position
    exactly (integers); numpy and this library compute (n-1)*(p/100) in
    doubles, so the interpolation weight carries an error of about n*eps and
    the result an error of about n*eps*span (span = distance of the two
    neighbouring order statistics). The deviation is therefore reported as a
    multiple of eps*(n*span + |ref|), not in ulp of the result."""
    worst = 0.0
    worst_where = None
    cases = 0
    over = 0

    @classmethod
    def add(cls, ours, ref, n, span, where, limit=2.0):
        cls.cases += 1
        if ours == ref:
            return
        den = EPS * (n * span + abs(ref)) + 4 * 5e-324   # + 4 denormal steps
        r = abs(ours - ref) / den if den > 0 else float("inf")
        if r > cls.worst:
            cls.worst, cls.worst_where = r, where
        if r > limit:
            cls.over += 1


def small_result(x):
    return x != 0.0 and abs(x) < 1e-290  # denormal range: ulp is meaningless


def check(inpath, outpath, cpath, strict):
    sets = read_sets(inpath)
    classes = open(cpath).read().split()
    raw = open(outpath, "rb").read()
    out = struct.unpack("<%dQ" % (len(raw) // 8), raw)
    assert len(out) == OUTW * len(sets), "output size %d != %d" % (len(out), OUTW * len(sets))
    st = Stat()
    fails = 0
    ncmp = 0
    skipped = []
    np_var_bad = 0
    np_var_n = 0
    hist_refused = 0
    for idx, (lo, hi, data) in enumerate(sets):
        fam = classes[idx]
        o = out[idx * OUTW:(idx + 1) * OUTW]
        f = [b2f(x) for x in o[:29]]
        n = len(data)
        if o[30] != 0:
            print("  FORM MISMATCH in set #%d (%s): %d differences between raw/Vec/inplace forms" % (idx, fam, o[30]))
            fails += 1
        # sum / mean
        try:
            ref_sum = math.fsum(data)
            sum_ok = not math.isinf(ref_sum)
        except OverflowError:
            ref_sum, sum_ok = float("inf"), False
        fe = [b2f(o[47]), b2f(o[48])]
        if sum_ok:
            st.add("sum", fam, f[0], ref_sum, idx)
            st.add("sum_exact", fam, fe[0], ref_sum, idx)
            st.add("mean/fmean", fam, f[1], statistics.fmean(data), idx)
            st.add("mean_exact/fmean", fam, fe[1], statistics.fmean(data), idx)
            if n <= 400:
                st.add("mean/exact", fam, f[1], statistics.mean(data), idx)
        st.add("min", fam, f[2], min(data), idx)
        st.add("max", fam, f[3], max(data), idx)
        # median
        st.add("median/statistics", fam, f[4], statistics.median(data), idx)
        if np is not None:
            arr = np.array(data, dtype=np.float64)
            with np.errstate(all="ignore"):
                st.add("median/numpy", fam, f[4], float(np.median(arr)), idx)
                ref = np.percentile(arr, PCTS)
            for k, p in enumerate(PCTS):
                st.add("percentile/numpy", fam, f[5 + k], float(ref[k]), idx)
        # quantiles inclusive: deciles
        q10 = statistics.quantiles(data, n=10, method="inclusive") if n >= 2 else [data[0]] * 9
        srt = sorted(data)
        for j, p in enumerate((10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0)):
            k = PCTS.index(p)
            lo_i = (n - 1) * (j + 1) // 10
            span = abs(srt[min(lo_i + 1, n - 1)] - srt[lo_i])
            if math.isinf(span):
                continue
            QuantileStat.add(f[5 + k], q10[j], n, span, (fam, idx))
        # variance family: n >= 2 statistics (ours NaN for n < 2)
        if n >= 2:
            try:
                rv = float(statistics.variance(data))
                rp = float(statistics.pvariance(data))
            except OverflowError:
                continue
            for name, ours, ref, basis in (
                    ("variance/statistics", f[25], rv, rv), ("pvariance", f[27], rp, rp),
                    ("stdev", f[26], math.sqrt(rv) if rv >= 0 and not math.isinf(rv) else rv, rv),
                    ("pstdev", f[28], math.sqrt(rp) if not math.isinf(rp) else rp, rp)):
                # results (or the variance behind a stdev) in the denormal
                # range carry fewer than 53 bits: ulp is meaningless there
                if small_result(ref) or small_result(ours) or small_result(basis) or math.isinf(ref):
                    skipped.append(name)
                    continue
                st.add(name, fam, ours, ref, idx)
            if np is not None:
                # numpy's own variance is the less accurate one (it cancels
                # against the mean): agreement is judged by a relative bound
                # that grows with mean^2 * eps, not in ulp
                with np.errstate(all="ignore"):
                    nv = float(np.var(arr, ddof=1))
                mean2 = statistics.fmean(data) ** 2
                bound = 1e-9 * max(abs(nv), abs(f[25])) + 64 * 2.220446049250313e-16 * mean2
                if not (abs(nv - f[25]) <= bound) and not math.isinf(mean2):
                    np_var_bad += 1
                    print("  variance vs numpy: set #%d (%s) ours=%r numpy=%r" % (idx, fam, f[25], nv))
                np_var_n += 1
        else:
            if not math.isnan(f[25]) or not math.isnan(f[26]):
                print("  expected NaN variance for n=%d in set #%d" % (n, idx))
                fails += 1
            if f[27] != 0.0 or f[28] != 0.0:
                print("  expected pvariance 0 for n=1 in set #%d" % idx)
                fails += 1
        # histogram
        if np is not None:
            try:
                href, _ = np.histogram(arr, bins=16, range=(lo, hi))
            except ValueError:   # numpy refuses ranges too small for 16 finite bins
                hist_refused += 1
                href = None
            if href is not None and (list(href) != list(o[31:47]) or int(o[29]) != int(sum(href))):
                print("  HISTOGRAM MISMATCH set #%d (%s) ours=%s numpy=%s" % (idx, fam, list(o[31:47]), list(href)))
                fails += 1
        ncmp += 1
    # ------------------------------------------------------------ report
    print("stats cross-check: %d data sets, numpy %s" % (len(sets), np.__version__ if np is not None else "NOT installed"))
    stats = sorted({k[0] for k in st.max_ulp})
    fams = sorted({k[1] for k in st.max_ulp})
    print("  max deviation in ulp (cases above tolerance in brackets), per statistic and family:")
    print("  %-22s" % "statistic" + "".join("%11s" % fm[:10] for fm in fams) + "%9s" % "ALL")
    over_total = 0
    gmax = 0
    for s in stats:
        row = "  %-22s" % s
        mx = 0
        for fm in fams:
            key = (s, fm)
            if key in st.max_ulp:
                u, o_ = st.max_ulp[key], st.over.get(key, 0)
                row += "%11s" % (("%d" % u) + ("(%d)" % o_ if o_ else ""))
                mx = max(mx, u)
                over_total += o_
            else:
                row += "%11s" % "-"
        gmax = max(gmax, mx) if not s.startswith("variance") and not s.endswith("stdev") else gmax
        print(row + "%9d" % mx)
    print("  worst case per statistic (ulp, family, set #):")
    for s in stats:
        print("    %-22s %s" % (s, st.worst.get(s)))
    allmax = {s: max([v for (k, v) in st.max_ulp.items() if k[0] == s]) for s in stats}
    print("  max ulp overall: " + ", ".join("%s=%d" % (s, allmax[s]) for s in stats))
    print("  variance results in the denormal range skipped (no ulp meaning): %d" % len(skipped))
    if np is not None:
        print("  variance vs numpy.var(ddof=1) (relative 1e-9 + 64 eps mean^2): %d sets, %d outside" % (np_var_n, np_var_bad))
        fails += np_var_bad
    print("  percentile vs statistics.quantiles(inclusive): %d values, worst |diff|/(eps*(n*span+|ref|)) = %.3f %s, above 2.0: %d"
          % (QuantileStat.cases, QuantileStat.worst, QuantileStat.worst_where, QuantileStat.over))
    over_total += QuantileStat.over
    if np is not None:
        print("  histogram: all counts equal numpy.histogram's (%d sets refused by numpy: range too small for 16 bins)" % hist_refused)
    print("  cases above the documented tolerance: %d, NaN mismatches: %d, form/histogram failures: %d"
          % (over_total, st.nan_bad, fails))
    ncases = sum(st.n.values()) + QuantileStat.cases
    print("  compared values: %d" % ncases)
    if fails or st.nan_bad or (strict and over_total):
        print("FAIL")
        return 1
    print("OK")
    return 0


if __name__ == "__main__":
    if len(sys.argv) >= 6 and sys.argv[1] == "gen":
        gen(int(sys.argv[2]), int(sys.argv[3]), sys.argv[4], sys.argv[5])
    elif len(sys.argv) >= 5 and sys.argv[1] == "check":
        sys.exit(check(sys.argv[2], sys.argv[3], sys.argv[4], "--strict" in sys.argv))
    else:
        print(__doc__)
        sys.exit(2)
