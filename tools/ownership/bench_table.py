import csv
import json
import sys


def num(v):
    try:
        return float(v)
    except ValueError:
        return 0.0


def ratio(a, b):
    return "-" if a == 0 else "%.3f" % (b / a)


def main():
    path = sys.argv[1]
    rows = list(csv.DictReader(open(path), delimiter="\t"))
    by = {}
    for r in rows:
        by.setdefault(r["target"], {})[r["mode"]] = r
    header = ["target", "Ir off", "Ir on", "Ir on/off", "user off", "user on", "user on/off", "rss off kB", "rss on kB", "gc runs off/on", "pause max off/on us", "pause sum off/on ms", "alloc off/on MB", "same output"]
    print("| " + " | ".join(header) + " |")
    print("|" + "|".join(["---"] * len(header)) + "|")
    out = []
    for t, modes in by.items():
        a = modes.get("off")
        b = modes.get("on")
        if a is None or b is None:
            continue
        if a["build_rc"] != "0" or b["build_rc"] != "0":
            print("| %s | build failed (off=%s on=%s) |" % (t, a["build_rc"], b["build_rc"]) + " |" * (len(header) - 2))
            continue
        same = "n/a" if a["deterministic"] != "1" else ("yes" if a["out_md5"] == b["out_md5"] and a["run_rc"] == b["run_rc"] else "NO")
        ir_a, ir_b = num(a["ir"]), num(b["ir"])
        u_a, u_b = num(a["user_s"]), num(b["user_s"])
        cells = [
            t,
            "%.1f M" % (ir_a / 1e6) if ir_a else "-",
            "%.1f M" % (ir_b / 1e6) if ir_b else "-",
            ratio(ir_a, ir_b),
            "%.2f" % u_a,
            "%.2f" % u_b,
            ratio(u_a, u_b),
            "%d" % num(a["rss_kb"]),
            "%d" % num(b["rss_kb"]),
            "%d / %d" % (num(a["gc_runs"]), num(b["gc_runs"])),
            "%.0f / %.0f" % (num(a["gc_pause_max_ns"]) / 1e3, num(b["gc_pause_max_ns"]) / 1e3),
            "%.2f / %.2f" % (num(a["gc_pause_total_ns"]) / 1e6, num(b["gc_pause_total_ns"]) / 1e6),
            "%.1f / %.1f" % (num(a["gc_alloc_bytes"]) / 1e6, num(b["gc_alloc_bytes"]) / 1e6),
            same,
        ]
        print("| " + " | ".join(cells) + " |")
        out.append({"target": t, "off": a, "on": b, "same_output": same})
    json.dump(out, open(path.replace(".tsv", ".json"), "w"), indent=1)


main()
