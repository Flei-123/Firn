#!/usr/bin/env python3
import argparse
import json
import os
import sys
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from designlint_lib import allow, engine
from designlint_lib.rules import REGISTRY

DEFAULT_ALLOW = os.path.join(HERE, "designlint_allow.txt")
DEFAULT_PATHS = ["examples/fui_apps"]


def main(argv=None):
    ap = argparse.ArgumentParser(description="fUi design lint")
    ap.add_argument("paths", nargs="*", default=None)
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--allow", default=DEFAULT_ALLOW)
    ap.add_argument("--no-allow", action="store_true")
    ap.add_argument("--rules", default="")
    ap.add_argument("--list-rules", action="store_true")
    ap.add_argument("--counts", action="store_true")
    args = ap.parse_args(argv)

    if args.list_rules:
        for rule_id, summary, _fn in REGISTRY:
            print(rule_id + "\t" + summary)
        return 0

    rules = {r for r in args.rules.split(",") if r}
    paths = args.paths or DEFAULT_PATHS
    found = engine.lint_paths(paths, rules)
    entries = [] if args.no_allow else allow.load(args.allow)
    kept, skipped = engine.split_allowed(found, entries)
    counts = Counter(f["rule"] for f in kept)

    if args.json:
        print(json.dumps({"findings": kept, "allowed": len(skipped), "counts": dict(counts)}, indent=2, ensure_ascii=False))
    else:
        for f in kept:
            print("%s:%d: %s: %s [%s]" % (f["file"], f["line"], f["rule"], f["message"], f["text"]))
        if args.counts or kept:
            for rule_id, _s, _fn in REGISTRY:
                print("%-20s %d" % (rule_id, counts.get(rule_id, 0)))
            print("allowed: %d" % len(skipped))
    return 1 if kept else 0


if __name__ == "__main__":
    sys.exit(main())
