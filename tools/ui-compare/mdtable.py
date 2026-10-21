#!/usr/bin/env python3
"""Markdown results tables from results/*.json + results/compare.json (only real numbers; '-' = not measured).
Usage: mdtable.py > tables.md"""
import json
import os

HERE = os.environ.get("UIVGL_DIR", os.path.dirname(os.path.abspath(__file__)))
RES = os.path.join(HERE, "results")
APPS = ["flei", "chat", "settings", "dash", "files", "form", "notes"]
TKS = ["original", "fui", "web", "egui", "gtk", "qt"]


def jl(p):
    try:
        with open(p) as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def med(d, nd=0, div=1.0):
    if isinstance(d, dict) and d.get("median") is not None:
        return ("%." + str(nd) + "f") % (d["median"] / div)
    return "-"


def rng(d, nd=0, div=1.0):
    if isinstance(d, dict) and d.get("median") is not None:
        f = "%." + str(nd) + "f"
        return (f + " (" + f + "-" + f + ")") % (d["median"] / div, d["min"] / div, d["max"] / div)
    return "-"


def main():
    cmp_rows = jl(os.path.join(RES, "compare.json")) or []
    out = []
    metrics = [
        ("Lines of code (non-empty, non-comment)", lambda r: str(r["lines"]["total"]) if r.get("lines") else "-"),
        ("Binary / bundle size (MB)", lambda r: "%.2f" % (r["size_bytes"] / 1e6) if r.get("size_bytes") else "-"),
        ("Build time (s, clean)", lambda r: "%.0f" % r["build_s"] if r.get("build_s") is not None and r.get("build_rc") == 0 else "-"),
        ("Startup to 1st frame (ms, median)", lambda r: rng(r.get("startup_ms"))),
        ("RAM idle PSS (MB, median)", lambda r: rng(r.get("ram_pss_kb"), 0, 1024.0)),
        ("CPU idle (% of one core)", lambda r: rng(r.get("cpu_idle_pct"), 1)),
        ("Input -> pixel latency (ms)", lambda r: rng(r.get("input_latency_ms"), 1)),
    ]
    for name, fn in metrics:
        out.append("### " + name)
        out.append("| app | " + " | ".join(TKS) + " |")
        out.append("|---|" + "---|" * len(TKS))
        for app in APPS:
            cells = []
            for tk in TKS:
                r = jl(os.path.join(RES, "%s-%s.json" % (app, tk)))
                try:
                    cells.append(fn(r) if r else "-")
                except (KeyError, TypeError):
                    cells.append("-")
            out.append("| %s | %s |" % (app, " | ".join(cells)))
        out.append("")
    rt = jl(os.path.join(RES, "chat-web.json")) or {}
    if rt.get("runtime_size_bytes"):
        out.append("Runtime (Chromium, /usr/lib/chromium): %.0f MB (not part of the Web bundle size above)." % (rt["runtime_size_bytes"] / 1e6))
        out.append("")
    out.append("### Visual similarity (SSIM / mean abs. deviation %) vs reference (flei: original Svelte, others: web)")
    out.append("| app | theme | " + " | ".join(t for t in TKS if t != "original") + " |")
    out.append("|---|---|" + "---|" * len([t for t in TKS if t != "original"]))
    for app in APPS:
        for theme in ("dark", "light"):
            cells = []
            for tk in [t for t in TKS if t != "original"]:
                m = [c for c in cmp_rows if c.get("app") == app and c.get("toolkit") == tk and c.get("theme") == theme and "ssim" in c]
                cells.append("%.3f / %.2f" % (m[0]["ssim"], m[0]["mad_pct"]) if m else "-")
            out.append("| %s | %s | %s |" % (app, theme, " | ".join(cells)))
    print("\n".join(out))


if __name__ == "__main__":
    main()
