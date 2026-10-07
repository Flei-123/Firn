#!/usr/bin/env python3
"""adopt.py -- switch shell test scripts from their own ok()/bad() copies to the shared testkit.sh.

    python3 adopt.py [--dry-run] FILE.sh ...

Only the exact form that Osum/Certus/Firn copied ~150 times is replaced (counters `pass`/`fail`, lines
"  OK    label" / "  FAIL  label"); every other script is left alone and reported. The two definition lines are
replaced by one line that sources the kit, at the place of the first definition:
    . "${FIRN:-/root/firn}/tools/testkit/testkit.sh"
Nothing else in the script changes, so its output stays byte for byte the same.
"""
import re
import sys

OK = re.compile(r"^ok\(\)\s*\{ pass=\$\(\(pass ?\+ ?1\)\); printf '  OK    %s\\n' \"\$1\"; \}[ \t]*\n", re.M)
BAD = re.compile(r"^bad\(\)\s*\{ fail=\$\(\(fail ?\+ ?1\)\); printf '  FAIL  %s\\n' \"\$1\"; \}[ \t]*\n", re.M)
SRC = '. "${FIRN:-/root/firn}/tools/testkit/testkit.sh"\n'


def adopt(text):
    m1, m2 = OK.search(text), BAD.search(text)
    if not (m1 and m2) or len(OK.findall(text)) != 1 or len(BAD.findall(text)) != 1:
        return None
    first = min(m1.start(), m2.start())
    out = OK.sub("", text, 1)
    out = BAD.sub("", out, 1)
    return out[:first] + SRC + out[first:]


def main():
    dry = "--dry-run" in sys.argv
    n = 0
    for p in (a for a in sys.argv[1:] if not a.startswith("--")):
        t = open(p, encoding="utf8").read()
        r = adopt(t)
        if r is None:
            print("skip   %s (not the exact form)" % p)
            continue
        n += 1
        print("%s %s" % ("would " if dry else "adopted", p))
        if not dry:
            open(p, "w", encoding="utf8").write(r)
    print("%d script(s)" % n)


if __name__ == "__main__":
    main()
