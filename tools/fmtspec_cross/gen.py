#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/fmtspec_cross/gen.py SEED N OUT.fi OUT.expect
# Random f-string format specs: writes a Firn program (one f-string per case) and the
# output Python's format() gives for the same value and spec.
import random
import sys

seed, n, out_fi, out_exp = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3], sys.argv[4]
rnd = random.Random(seed)
FILLS = ["", "*", "-", "x", "0", "#", "_", ".", "中", "ä", "€"]
ALIGN = ["", "<", ">", "^"]
STRS = ["", "a", "ab", "hello", "hällo", "中文", "x y z", "0123456789"]
# r708: texts for the `!r` cases (quotes and backslashes change the repr)
REPR_STRS = STRS + ["it's", 'say "hi"', 'both \' and "', "a\\b", "tab\there", "line\nbreak"]


def spec_common(num: bool, types: str, alt_ok: bool, prec_ok: bool, str_kind: bool, group: str = ""):
    """Returns (firn_spec, python_spec); nested width/precision fields use @W@ / @P@."""
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
    fs, ps = s, s
    if rnd.random() < 0.8:
        w = str(rnd.choice([1, 2, 3, 5, 8, 10, 12, 20]))
        if rnd.random() < 0.25:
            fs += "@W@:" + w
            ps += w
        else:
            fs += w
            ps += w
    if group:
        g = rnd.choice(group)
        fs += g
        ps += g
    if prec_ok and rnd.random() < 0.7:
        pr = str(rnd.choice([0, 1, 2, 3, 4, 6, 8, 12]))
        if rnd.random() < 0.25:
            fs += ".@P@:" + pr
        else:
            fs += "." + pr
        ps += "." + pr
    if types:
        t = rnd.choice(types)
        fs += t
        ps += t
    return fs, ps


cases = []
FLOATTY = ["f", "f", "F", "%", "e", "E", "g", "G", "n", ""]
for _ in range(n):
    k = rnd.choice("iiufffsb")
    if k == "i" or k == "u":
        v = rnd.choice([0, 1, -1, 7, 42, 255, 1000, -999, 65535, 123456789, -123456789,
                        rnd.randint(-10**6, 10**6), rnd.randint(0, 2**40), rnd.randint(0, 2**62)])
        if k == "u":
            v = abs(v)
        t = rnd.choice(["", "d", "x", "X", "o", "b", "f", "%", "e", "E", "g", "G", "n", "d", "c"])
        if t in ("f", "%") and abs(v) > 2**50:
            v = v % 10**9
        if t == "c":
            v = rnd.choice([65, 97, 233, 8364, 20013, 0x1F600, 48, 126])
            fs, ps = spec_common(False, "c", False, False, False)
            fs = fs.replace("@W@:", "@W@:")
            if k == "i" or k == "u":
                # numbers default to right alignment for `c`
                pass
        else:
            grp = ""
            if t in ("", "d", "f", "%", "e", "E", "g", "G"):
                grp = rnd.choice(["", "", ",", "_"])
            elif t in ("x", "X", "o", "b"):
                grp = rnd.choice(["", "", "_"])
            fs, ps = spec_common(True, t, t in ("x", "X", "o", "b"),
                                 t in ("f", "%", "e", "E", "g", "G"), False, grp)
        cases.append(("i64" if k == "i" else "u64", str(v), v, fs, ps))
    elif k == "f":
        mode = rnd.random()
        if mode < 0.5:
            v = rnd.choice([0.0, 0.5, -0.5, 1.5, 2.5, 2.675, 0.125, 3.14159, -2.71828, 1234.5678,
                            round(rnd.uniform(-1000, 1000), 3), rnd.uniform(-1e6, 1e6),
                            rnd.uniform(0, 1), 1e9, 0.001])
        else:
            v = rnd.choice([-1, 1]) * rnd.uniform(1, 10) * 10.0 ** rnd.randint(-40, 40)
            v = rnd.choice([v, 5e-324, 1.7976931348623157e308, 9.9995, 99999.5, 0.0001, 1e22, 1e16, 123456789.0])
        t = rnd.choice(FLOATTY)
        if t == "" and not (isinstance(v, float) and abs(v) < 1e15 and (v == 0 or abs(v) >= 1e-4)
                            and len(repr(float(v))) < 18 and "e" not in repr(float(v))):
            t = rnd.choice(["g", "e"])
        if t in ("f", "F", "%") and abs(v) > 1e15:
            t = "e"
        grp = rnd.choice(["", "", ",", "_"]) if t != "n" else ""
        has_prec = t != ""
        fs, ps = spec_common(True, t, False, has_prec, False, grp)
        cases.append(("f64", repr(float(v)), float(v), fs, ps))
    elif k == "s":
        use_repr = rnd.random() < 0.35
        v = rnd.choice(REPR_STRS if use_repr else STRS)
        t = rnd.choice(["", "s"])
        fs, ps = spec_common(False, t, False, True, True)
        lit = '"' + v.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t") + '"'
        cases.append(("strr" if use_repr else "str", lit, v, fs, ps))
    else:
        v = rnd.choice([True, False])
        fs, ps = spec_common(False, "", False, False, True)
        cases.append(("bool", "true" if v else "false", v, fs, ps))

fi = ["import std.io", "import std.rt", "", "fn show(s: str) {", "    io.print_line(s)", "}", ""]
exp = []
per = 40
for chunk in range(0, len(cases), per):
    fi.append("fn part%d() {" % (chunk // per))
    for i, (ty, lit, v, fspec, pspec) in enumerate(cases[chunk:chunk + per]):
        # a spec with a '"' or '{' cannot occur (generator alphabet) except the nested fields
        fi.append("    let v%d: %s = %s" % (i, "str" if ty == "strr" else ty, lit))
        # nested fields: @W@:<digits> -> {wN}, @P@:<digits> -> {pN}
        import re
        m = re.search(r"@W@:(\d+)", fspec)
        if m:
            fi.append("    let w%d: i32 = %s" % (i, m.group(1)))
            fspec = fspec.replace(m.group(0), "{w%d}" % i)
        m = re.search(r"@P@:(\d+)", fspec)
        if m:
            fi.append("    let p%d: u8 = %s" % (i, m.group(1)))
            fspec = fspec.replace(m.group(0), "{p%d}" % i)
        fi.append('    show(io.fmt_string(f"[{v%d%s:%s}]"))' % (i, "!r" if ty == "strr" else "", fspec))
        if ty == "strr":
            exp.append("[" + format(repr(v), pspec) + "]")
        elif ty == "bool":
            exp.append("[" + format(str(v).lower(), pspec) + "]")
        elif ty == "i64" and "c" in pspec[-1:]:
            exp.append("[" + format(v, pspec) + "]")
        else:
            exp.append("[" + format(v, pspec) + "]")
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
