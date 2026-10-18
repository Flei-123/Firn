#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fmtspec_cross/gen.py SEED N OUT.fi OUT.expect
# Random f-string format specs: writes a Firn program (one f-string per case) and the
# output Python's format() gives for the same value and spec.
import random
import sys

seed, n, out_fi, out_exp = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4]
rnd = random.Random(seed)
FILLS = ["", "*", "-", "x", "0", "#", "_", "."]
ALIGN = ["", "<", ">", "^"]
STRS = ["", "a", "ab", "hello", "hällo", "中文", "x y z", "0123456789"]


def spec_common(num: bool, types: str, alt_ok: bool, prec_ok: bool, str_kind: bool) -> str:
    s = ""
    al = rnd.choice(ALIGN)
    if al and rnd.random() < 0.6:
        s += rnd.choice(FILLS) + al
    elif al:
        s += al
    elif num and rnd.random() < 0.2:
        s += rnd.choice(FILLS) + "="
    if num:
        s += rnd.choice(["", "", "+", " ", "-"])
        if alt_ok and rnd.random() < 0.3:
            s += "#"
        if rnd.random() < 0.3:
            s += "0"
    if rnd.random() < 0.8:
        s += str(rnd.choice([1, 2, 3, 5, 8, 10, 12, 20]))
    if prec_ok and rnd.random() < 0.7:
        s += "." + str(rnd.choice([0, 1, 2, 3, 4, 6, 8]))
    if types:
        s += rnd.choice(types)
    return s


cases = []
for _ in range(n):
    k = rnd.choice("iufsb")
    if k == "i" or k == "u":
        v = rnd.choice([0, 1, -1, 7, 42, 255, 1000, -999, 65535, 123456789, -123456789,
                        rnd.randint(-10**6, 10**6), rnd.randint(0, 2**40)])
        if k == "u":
            v = abs(v)
        t = rnd.choice(["", "d", "x", "X", "o", "b", "f", "%"])
        spec = spec_common(True, t, t in ("x", "X", "o", "b"), t in ("f", "%"), False)
        # an integer without type takes no precision
        cases.append(("i64" if k == "i" else "u64", str(v), v, spec))
    elif k == "f":
        v = rnd.choice([0.0, 0.5, -0.5, 1.5, 2.5, 2.675, 0.125, 3.14159, -2.71828, 1234.5678,
                        round(rnd.uniform(-1000, 1000), 3), rnd.uniform(-1e6, 1e6),
                        rnd.uniform(0, 1), 1e9, 0.001])
        t = rnd.choice(["f", "f", "F", "%", ""])
        has_prec = t != ""
        spec = spec_common(True, t, False, has_prec, False)
        if t == "":
            # shortest-repr floats only for plain values (no exponent form)
            v = round(rnd.uniform(-1000, 1000), 3)
        cases.append(("f64", repr(float(v)), float(v), spec))
    elif k == "s":
        v = rnd.choice(STRS)
        t = rnd.choice(["", "s"])
        spec = spec_common(False, t, False, True, True)
        lit = '"' + v + '"'
        cases.append(("str", lit, v, spec))
    else:
        v = rnd.choice([True, False])
        spec = spec_common(False, "", False, False, True)
        cases.append(("bool", "true" if v else "false", v, spec))

fi = ["import std.io", "import std.rt", "", "fn show(s: str) {", "    io.print_line(s)", "}", ""]
exp = []
per = 40
for chunk in range(0, len(cases), per):
    fi.append("fn part%d() {" % (chunk // per))
    for i, (ty, lit, v, spec) in enumerate(cases[chunk:chunk + per]):
        # a spec with a '"' or '{' cannot occur (generator alphabet)
        fi.append("    let v%d: %s = %s" % (i, ty, lit))
        fi.append('    show(io.fmt_string(f"[{v%d:%s}]"))' % (i, spec))
        if ty == "bool":
            exp.append("[" + format(str(v).lower(), spec) + "]")
        else:
            exp.append("[" + format(v, spec) + "]")
    fi.append("}")
    fi.append("")
fi.append("fn main() -> i32 {")
fi.append("    if !gc_init() { return 1 }")
for c in range(0, (len(cases) + per - 1) // per):
    fi.append("    part%d()" % c)
fi.append("    return 0")
fi.append("}")
open(out_fi, "w", encoding="utf-8").write("\n".join(fi) + "\n")
open(out_exp, "w", encoding="utf-8").write("\n".join(exp) + "\n")
