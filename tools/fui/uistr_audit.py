#!/usr/bin/env python3
"""Audit of the localised standard texts (gap list #64): exit 1 on any finding.

  1. no `fn tr_x() -> str { return "literal" }` is left in lib/fui (every text goes through uistr.t);
  2. every key used in `uistr.t("key", ...)` has a German entry in lib/fui/uistr.fi;
  3. no German entry is dead (its key is used nowhere).
Run from the repo root: python3 tools/fui/uistr_audit.py
"""
import re, sys, glob

bare = re.compile(r'^fn tr_\w+\(\) -> str \{ return "')
used = {}
bad = 0
for p in sorted(glob.glob("lib/fui/*.fi")):
    if p.endswith("uistr.fi"):
        continue
    for n, line in enumerate(open(p, encoding="utf-8"), 1):
        if bare.match(line):
            print("BARE LITERAL %s:%d %s" % (p, n, line.strip()[:70]))
            bad += 1
        for m in re.finditer(r'uistr\.t\("([^"]+)"', line):
            used[m.group(1)] = (p, n)
src = open("lib/fui/uistr.fi", encoding="utf-8").read()
de = set(re.findall(r'if same\(key, "([^"]+)"\)', src))
for k, (p, n) in sorted(used.items()):
    if k not in de:
        print("NO GERMAN %s (%s:%d)" % (k, p, n))
        bad += 1
for k in sorted(de - set(used)):
    print("DEAD GERMAN %s" % k)
    bad += 1
print("uistr audit: %d keys used, %d German entries, %d findings" % (len(used), len(de), bad))
sys.exit(1 if bad else 0)
