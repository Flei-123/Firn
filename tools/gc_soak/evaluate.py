#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/gc_soak/evaluate.py -- reads the TSV of tools/gc_soak/soak.fi.

Three modes:

    evaluate.py <mode0.tsv> <mode1.tsv> <min_ms>
        the pair: the run that has to stay bounded, and the counter-check
        that has to grow. A measurement that cannot strike is worth nothing.

    evaluate.py --single <tsv> <min_ms>
        one long series: the real duration, RSS and heap over the quarters,
        the longest pause per quarter, the ratio heap to live.

    evaluate.py --ab <with.tsv> <without.tsv>
        the two sides of the rescan A/B.

Columns (see the head of the file):
    t_ms rounds rss_kib live heap_bytes live_bytes collections
    pause_win_ns mark_win_ns sweep_win_ns stop_win_ns full_runs overflows asked
"""
import sys

T, ROUNDS, RSS, LIVE, HEAP, LIVEB, RUNS, PAUSE, MARK, SWEEP, STOP, FULL, OV, ASKED = range(14)


def read(path):
    rows, done = [], None
    for line in open(path, encoding="utf-8"):
        line = line.strip()
        if not line:
            continue
        if line.startswith("#"):
            if line.startswith("# done"):
                done = line
            if line.startswith("# error"):
                raise SystemExit("%s: %s" % (path, line))
            continue
        f = line.split("\t")
        if len(f) != 14:
            raise SystemExit("%s: a line with %d fields instead of 14" % (path, len(f)))
        rows.append([int(x) for x in f])
    if len(rows) < 8:
        raise SystemExit("%s: only %d samples" % (path, len(rows)))
    return rows, done


def median(xs):
    s = sorted(xs)
    n = len(s)
    if not n:
        return 0
    return s[n // 2] if n % 2 else (s[n // 2 - 1] + s[n // 2]) / 2


def ratiol_grew(second, last):
    a = median([r[HEAP] / r[LIVEB] for r in second if r[LIVEB] > 0] or [0])
    b = median([r[HEAP] / r[LIVEB] for r in last if r[LIVEB] > 0] or [0])
    return a > 0 and b > a * 1.10


def look(path):
    rows, done = read(path)
    q = len(rows) // 4
    warm = rows[q:]
    second = rows[q:2 * q] or rows[q:q + 1]
    last = rows[3 * q:] or rows[-1:]
    rss_after = [r[RSS] for r in warm]
    monotone = (all(b >= a for a, b in zip(rss_after, rss_after[1:]))
                and rss_after[-1] > rss_after[0])
    rss2, rssl = median([r[RSS] for r in second]), median([r[RSS] for r in last])
    heap2, heapl = median([r[HEAP] for r in second]), median([r[HEAP] for r in last])
    ratios = [r[HEAP] / r[LIVEB] for r in warm if r[LIVEB] > 0]
    return {
        "rows": rows, "done": done, "n": len(rows), "q": q,
        "ms": rows[-1][T], "rounds": rows[-1][ROUNDS], "asked": rows[-1][ASKED],
        "rss2": rss2, "rssl": rssl, "rss_max": max(r[RSS] for r in rows),
        "heap2": heap2, "heapl": heapl, "heap_max": max(r[HEAP] for r in rows),
        "live_max": max(r[LIVEB] for r in rows),
        "ratio_med": median(ratios) if ratios else 0,
        "ratio_max": max(ratios) if ratios else 0,
        "runs": rows[-1][RUNS], "full": rows[-1][FULL], "ov": rows[-1][OV],
        "pause": max(r[PAUSE] for r in rows),
        "stop": max(r[STOP] for r in rows),
        "mark": max(r[MARK] for r in rows),
        "sweep": max(r[SWEEP] for r in rows),
        "ratio2": median([r[HEAP] / r[LIVEB] for r in second if r[LIVEB] > 0] or [0]),
        "ratiol": median([r[HEAP] / r[LIVEB] for r in last if r[LIVEB] > 0] or [0]),
        "pause_med": median([r[PAUSE] for r in warm]),
        "pause_p95": sorted(r[PAUSE] for r in warm)[int(len(warm) * 0.95)],
        "stop_med": median([r[STOP] for r in warm]),
        "stop_p95": sorted(r[STOP] for r in warm)[int(len(warm) * 0.95)],
        # A leak grows by orders of magnitude -- the counter-check of this
        # very script shows a factor of well over ten within a minute. The
        # working set here BREATHES (the wave phase keeps up to 1,000
        # objects and then drops them all), so a few per cent between two
        # medians is the sampling, not a leak. The threshold is therefore
        # 1.25x and not 1.05x as in tools/gc_frag/run.sh, where the working
        # set is constant -- and the number that really answers the
        # fragmentation question stands next to it: heap divided by live.
        "grew": rssl > rss2 * 1.25 or heapl > heap2 * 1.25,
        "fragged": ratiol_grew(second, last),
        "monotone": monotone,
    }


def hms(ms):
    s = ms // 1000
    return "%dh %02dm %02ds" % (s // 3600, s % 3600 // 60, s % 60)


def show(u, name):
    print("   %s:" % name)
    print("     duration                %s (%d samples)" % (hms(u["ms"]), u["n"]))
    print("     rounds                  %d" % u["rounds"])
    print("     octets asked for        %d  (%.1f GiB through the allocator)"
          % (u["asked"], u["asked"] / 1073741824))
    print("     RSS median 2nd quarter  %d KiB" % u["rss2"])
    print("     RSS median last quarter %d KiB" % u["rssl"])
    print("     RSS maximum             %d KiB" % u["rss_max"])
    print("     heap median 2nd/last    %d / %d octets" % (u["heap2"], u["heapl"]))
    print("     heap maximum            %d octets" % u["heap_max"])
    print("     live maximum            %d octets" % u["live_max"])
    print("     heap/live median        %.2fx   maximum %.2fx"
          % (u["ratio_med"], u["ratio_max"]))
    print("     collections             %d (%d of them full), mark stack overflows %d"
          % (u["runs"], u["full"], u["ov"]))
    print("     longest pause           %.2f ms (whole interruption %.2f ms)"
          % (u["pause"] / 1e6, u["stop"] / 1e6))
    print("     pause per window        median %.2f ms, 95th %.2f ms "
          "(whole stop: median %.2f ms, 95th %.2f ms)"
          % (u["pause_med"] / 1e6, u["pause_p95"] / 1e6,
             u["stop_med"] / 1e6, u["stop_p95"] / 1e6))
    print("     heap/live 2nd -> last   %.2fx -> %.2fx  (fragmentation: %s)"
          % (u["ratio2"], u["ratiol"], "GROWS" if u["fragged"] else "flat"))
    print("     verdict                 %s (growth %s, monotone %s)"
          % ("GROWS" if (u["grew"] or u["monotone"]) else "bounded",
             "yes" if u["grew"] else "no", "yes" if u["monotone"] else "no"))


def quarters(u):
    """The longest pause per quarter -- do the pauses get longer over time?"""
    rows = u["rows"]
    n = len(rows)
    print("     per quarter of the run (this is what a monotone maximum hides):")
    print("       %-8s %-12s %-10s %-12s %-12s %s"
          % ("quarter", "duration", "RSS KiB", "heap/live", "pause ms", "stop ms"))
    for k in range(4):
        part = rows[k * n // 4:(k + 1) * n // 4]
        if not part:
            continue
        ratios = [r[HEAP] / r[LIVEB] for r in part if r[LIVEB] > 0]
        print("       %-8d %-12s %-10d %-12.2f %-12.2f %.2f"
              % (k + 1, hms(part[-1][T] - part[0][T]), median([r[RSS] for r in part]),
                 median(ratios) if ratios else 0,
                 max(r[PAUSE] for r in part) / 1e6,
                 max(r[STOP] for r in part) / 1e6))


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--single":
        u = look(sys.argv[2])
        min_ms = int(sys.argv[3]) if len(sys.argv) > 3 else 0
        show(u, "the endurance run (mode 0, changing object sizes)")
        quarters(u)
        if u["done"]:
            print("     %s" % u["done"][:150])
        else:
            print("     the run was still going when this was read -- the numbers")
            print("     above are the part that is on disk, not a forecast.")
        if u["ms"] < min_ms:
            print("     NOTE: below %d ms this is still the warm-up, no verdict." % min_ms)
            return 0
        if u["grew"] or u["monotone"] or u["fragged"]:
            print("     FAILED: RSS %d -> %d KiB, heap/live %.2fx -> %.2fx."
                  % (u["rss2"], u["rssl"], u["ratio2"], u["ratiol"]))
            return 1
        print("     PASSED: bounded. RSS %d -> %d KiB (median of the 2nd and of"
              % (u["rss2"], u["rssl"]))
        print("             the last quarter), heap/live %.2fx -> %.2fx."
              % (u["ratio2"], u["ratiol"]))
        return 0

    if len(sys.argv) > 1 and sys.argv[1] == "--ab":
        a = look(sys.argv[2])
        b = look(sys.argv[3])
        print("   The fix of 2026-08-24 (commit 47690a8a) scans the roots a SECOND")
        print("   time before the sweep. Both sides ran side by side, same load,")
        print("   same machine, same seed.")
        print("     %-26s %14s %14s" % ("", "with rescan", "without"))
        rows = [
            ("duration", hms(a["ms"]), hms(b["ms"])),
            ("rounds", "%d" % a["rounds"], "%d" % b["rounds"]),
            ("collections", "%d" % a["runs"], "%d" % b["runs"]),
            ("longest pause ms", "%.2f" % (a["pause"] / 1e6), "%.2f" % (b["pause"] / 1e6)),
            ("longest whole stop ms", "%.2f" % (a["stop"] / 1e6), "%.2f" % (b["stop"] / 1e6)),
            ("longest mark slice ms", "%.2f" % (a["mark"] / 1e6), "%.2f" % (b["mark"] / 1e6)),
            ("longest sweep slice ms", "%.2f" % (a["sweep"] / 1e6), "%.2f" % (b["sweep"] / 1e6)),
            ("RSS median last quarter", "%d KiB" % a["rssl"], "%d KiB" % b["rssl"]),
            ("heap/live median", "%.2fx" % a["ratio_med"], "%.2fx" % b["ratio_med"]),
        ]
        for name, x, y in rows:
            print("     %-26s %14s %14s" % (name, x, y))
        thr = (b["rounds"] / b["ms"]) / (a["rounds"] / a["ms"]) if a["ms"] and a["rounds"] else 0
        print("     throughput without / with : %.3fx" % thr)
        print("     per collection the rescan costs %.3f ms of mutator time"
              % (((a["ms"] - b["ms"] * a["rounds"] / b["rounds"]) / a["runs"])
                 if a["runs"] and b["rounds"] else 0))
        return 0

    a = look(sys.argv[1])
    b = look(sys.argv[2])
    min_ms = int(sys.argv[3]) if len(sys.argv) > 3 else 0
    show(a, "mode 0  replace (the working set stays constant)")
    show(b, "mode 1  keep    (counter-check, MUST grow)")
    print()
    errs = 0
    if a["ms"] < min_ms:
        print("   NOTE: mode 0 ran only %d ms; below %d ms the warm-up alone makes"
              % (a["ms"], min_ms))
        print("         the curve rise, so no verdict is given on it.")
    elif a["grew"] or a["monotone"] or a["fragged"]:
        print("   FAILED: with changing object sizes the heap grows without bound.")
        errs = 1
    else:
        print("   PASSED: with changing object sizes the heap stays bounded "
              "(RSS %d -> %d KiB, heap %d -> %d octets)."
              % (a["rss2"], a["rssl"], a["heap2"], a["heapl"]))
        print("   Overhead above the live set: %.2fx in the median, %.2fx at worst."
              % (a["ratio_med"], a["ratio_max"]))
    if not (b["grew"] or b["monotone"]):
        print("   FAILED: the counter-check does NOT grow -- the measurement is worthless.")
        errs = 1
    else:
        print("   Counter-check strikes: RSS %d -> %d KiB, heap up to %d octets."
              % (b["rss2"], b["rssl"], b["heap_max"]))
    return errs


if __name__ == "__main__":
    sys.exit(main())
