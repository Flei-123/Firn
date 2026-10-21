#!/usr/bin/env python3
"""Measure every app that has a config, then (run_all.sh) compare + gallery.

Config per app/toolkit: apps/<app>/<toolkit>/measure.json
  {
    "cmd":      "/abs/app --theme {theme} --shot {shot}",     (required)
    "run_cmd":  "/abs/app --theme {theme}",                    (optional)
    "src":      ["/abs/src/dir", "..."],                       (lines of code, counted by loc.py)
    "size_paths": ["/abs/binary_or_bundle_dir"],
    "runtime_size_paths": ["/usr/bin/chromium"],               (Web: browser as 'Runtime')
    "build_cmd": "cargo build --release",                      (optional, clean build is the worker's job)
    "input":    "click:550,350;key:Tab",                       (optional XTest specs)
    "cwd":      "/abs/dir"                                     (optional)
  }
Usage: run_all.py [--only app[,app]] [--toolkit tk[,tk]] [--runs 5] [--skip-build] [--only-metrics a,b]
Results: results/<app>-<toolkit>.json. Existing result files are overwritten.
"""
import argparse
import glob
import json
import os
import subprocess
import sys

SCRIPTS = os.path.dirname(os.path.abspath(__file__))
HERE = os.environ.get("UIVGL_DIR", SCRIPTS)  # data/shots/results/apps root


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--only", default="")
    ap.add_argument("--toolkit", default="")
    ap.add_argument("--runs", default="5")
    ap.add_argument("--skip-build", action="store_true")
    ap.add_argument("--metrics", default="", help="forwarded as --only to measure.py")
    a = ap.parse_args()
    apps = set(x for x in a.only.split(",") if x)
    tks = set(x for x in a.toolkit.split(",") if x)
    cfgs = sorted(glob.glob(os.path.join(HERE, "apps", "*", "*", "measure.json")))
    # web apps without their own measure.json get a default one (Chromium via webapp.sh)
    have = {os.path.dirname(c) for c in cfgs}
    for d in sorted(glob.glob(os.path.join(HERE, "apps", "*", "web"))):
        if d not in have and os.path.exists(os.path.join(d, "index.html")):
            cfg = {"cmd": "%s %s {theme} {shot}" % (os.path.join(SCRIPTS, "webapp.sh"), os.path.basename(os.path.dirname(d))),
                   "run_cmd": "%s %s {theme}" % (os.path.join(SCRIPTS, "webapp.sh"), os.path.basename(os.path.dirname(d))),
                   "src": [d], "size_paths": [d], "runtime_size_paths": ["/usr/lib/chromium"],
                   "input": "key:Tab"}
            with open(os.path.join(d, "measure.json"), "w") as f:
                json.dump(cfg, f, indent=1)
            cfgs.append(os.path.join(d, "measure.json"))
    cfgs.sort()
    if not cfgs:
        print("no apps/*/*/measure.json found yet - nothing to measure")
        return 0
    fails = 0
    for c in cfgs:
        tk = os.path.basename(os.path.dirname(c))
        app = os.path.basename(os.path.dirname(os.path.dirname(c)))
        if (apps and app not in apps) or (tks and tk not in tks):
            continue
        with open(c) as f:
            cfg = json.load(f)
        cmd = [sys.executable, os.path.join(SCRIPTS, "measure.py"), "--app", app, "--toolkit", tk,
               "--cmd", cfg["cmd"], "--runs", a.runs]
        if cfg.get("run_cmd"):
            cmd += ["--run-cmd", cfg["run_cmd"]]
        if cfg.get("src"):
            cmd += ["--src"] + cfg["src"]
        if cfg.get("size_paths"):
            cmd += ["--size-paths"] + cfg["size_paths"]
        if cfg.get("runtime_size_paths"):
            cmd += ["--runtime-size-paths"] + cfg["runtime_size_paths"]
        if cfg.get("build_cmd") and not a.skip_build:
            cmd += ["--build-cmd", cfg["build_cmd"]]
        if cfg.get("input"):
            cmd += ["--input", cfg["input"]]
        if cfg.get("cwd"):
            cmd += ["--cwd", cfg["cwd"]]
        if a.metrics:
            cmd += ["--only", a.metrics]
        print("== %s / %s" % (app, tk), flush=True)
        r = subprocess.run(cmd)
        if r.returncode != 0:
            print("!! measure failed for %s/%s (rc=%d)" % (app, tk, r.returncode))
            fails += 1
    return 1 if fails else 0


if __name__ == "__main__":
    sys.exit(main())
