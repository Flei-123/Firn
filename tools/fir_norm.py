#!/usr/bin/env python3
"""tools/fir_norm.py -- compare two `fir-raw` texts of a MODULE program up to the known cosmetic differences.

usage: fir_norm.py <firnc0.fir> <firnc1.fir>
exit 0 = same after normalising, 1 = different (the names of the different functions go to stdout).

What is normalised (all of it is order or spelling, never behaviour):
  * the function ORDER (firnc0 lowers the root file first, firnc1 the root file last), so functions are sorted by name;
  * the build path in panic messages (firnc0 spells `<exe dir>/../../../lib/..`, firnc1 `lib/..`);
  * the numbering of `@text#N` (assigned in lowering order): renumbered by first use in the sorted text.
Nothing is ignored any more: r767 (`gc T { .. } catch ..` numbering, slot = the `__val` cell) and r768
(`ARCH_GET_TCB` -> `ARCH_GET_FS` in the lowering) are the same in firnc1 now.
"""
import re
import sys

IGNORED = set()


def split(text):
    parts = re.split(r"(?m)^(?=fn @)", text)
    return parts[0], {p.split("(")[0]: p for p in parts[1:]}


def norm(path):
    text = open(path).read()
    text = re.sub(r"( at )[^\"\n]*?/lib/", r"\1lib/", text)
    head, fns = split(text)
    body = "".join(fns[k] for k in sorted(fns) if k not in IGNORED)
    names = {}

    def renumber(m):
        return names.setdefault(m.group(0), "@text#%d" % len(names))

    body = re.sub(r"@text#\d+", renumber, body)
    return head, split(body)[1], len(fns)


ha, fa, na = norm(sys.argv[1])
hb, fb, nb = norm(sys.argv[2])
bad = sorted(set(k for k in fa if fa[k] != fb.get(k)) | set(k for k in fb if k not in fa))
if ha != hb or bad or na != nb:
    print(" ".join(k[4:] for k in bad[:6]) + (" ..." if len(bad) > 6 else ""))
    sys.exit(1)
sys.exit(0)
