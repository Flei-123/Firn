#!/usr/bin/env python3
"""ROUND PHI -- a generator for bool/phi heavy programs.

WHY A GENERATOR AND NOT MORE HAND WRITTEN TESTS.  The bug round INLINE found
needs FOUR things to line up at once:

  1. a callee with SEVERAL `ret`s, so `inline.rs` returns its value through a
     slot and the continuation block begins with `load.bool`,
  2. that continuation block having EXACTLY ONE instruction and ending in
     `brcond` on the loaded value -- `fork_at`'s pattern,
  3. the caller naming the loaded value AGAIN further on, typically in the
     phi that `mem2reg` builds for a variable assigned in both arms,
  4. the inliner's budget actually reaching that call site.

Hand writing that shape is how 1136 came about.  Hand writing the HUNDRED
neighbouring shapes -- two callees, three returns, the value used in a
nested loop, the guard inverted, the merge through `&&` instead of `if` --
is what a generator is for.

EVERY generated program CHECKS ITSELF.  The generator computes the expected
answer in Python, over the same integers, and emits `return <n>` on
mismatch.  So the program is its own oracle: exit 0 is right, anything else
names the assertion that broke.  That makes the four level gate and the
differential test both applicable without a golden file.

Usage:  genbool.py <outdir> <count> [seed]
"""
import os
import random
import sys


def gen_callee(r, name, nret):
    """A callee with `nret` `ret`s -- the shape that makes inline.rs use a slot."""
    lines = [f"fn {name}(k: u64, cap: u64) -> bool {{"]
    guards = []
    for i in range(nret - 1):
        style = r.randrange(3)
        if style == 0:
            c, res = f"k == {r.randrange(0, 4)}", r.choice(["false", "true"])
            lines.append(f"    if {c} {{ return {res} }}")
            guards.append(("eq", r.randrange(0, 4), res))
            guards[-1] = ("eq", int(c.split("== ")[1]), res)
        elif style == 1:
            n = r.randrange(1, 5)
            res = r.choice(["false", "true"])
            lines.append(f"    if cap < {n} {{ return {res} }}")
            guards.append(("caplt", n, res))
        else:
            n = r.randrange(5, 40)
            res = r.choice(["false", "true"])
            lines.append(f"    if k > {n} {{ return {res} }}")
            guards.append(("kgt", n, res))
    tail = r.choice(["k == cap", "k < cap", "k != cap"])
    lines.append(f"    return {tail}")
    lines.append("}")
    return "\n".join(lines), guards, tail


def eval_callee(guards, tail, k, cap):
    for kind, n, res in guards:
        if kind == "eq" and k == n:
            return res == "true"
        if kind == "caplt" and cap < n:
            return res == "true"
        if kind == "kgt" and k > n:
            return res == "true"
    if tail == "k == cap":
        return k == cap
    if tail == "k < cap":
        return k < cap
    return k != cap


def gen_outer(r, oname, callees, shape):
    """The caller: several embedded calls, the later ones guarded by the
    result of the earlier, so the merge really becomes a phi."""
    c0 = callees[0][0]
    c1 = callees[1 % len(callees)][0]
    body = [f"fn {oname}(a: u64, b: u64, c: u64, d: u64) -> bool {{"]
    if shape == 0:
        body += [
            f"    var have: bool = {c0}(a, b)",
            "    if !have {",
            f"        if c != 0 && {c1}(c, b) {{ have = true }}",
            "    }",
            "    return have",
        ]
    elif shape == 1:
        body += [
            f"    var have: bool = {c0}(a, b)",
            "    if have {",
            f"        if d != 0 {{ have = {c1}(d, b) }}",
            "    }",
            "    return have",
        ]
    elif shape == 2:
        body += [
            f"    var have: bool = {c0}(a, b) || {c1}(c, b)",
            "    if d != 0 && !have { have = true }",
            "    return have",
        ]
    elif shape == 3:
        body += [
            "    var have: bool = false",
            "    var i: u64 = 0",
            "    while i < d {",
            f"        if {c0}(a + i, b) {{ have = true }}",
            "        i = i + 1",
            "    }",
            f"    if !have {{ have = {c1}(c, b) }}",
            "    return have",
        ]
    else:
        body += [
            f"    var x: bool = {c0}(a, b)",
            f"    var y: bool = {c1}(c, b)",
            "    var have: bool = false",
            "    if x { have = true } else { if y && d != 0 { have = true } }",
            "    return have",
        ]
    body.append("}")
    return "\n".join(body)


def eval_outer(shape, cs, a, b, c, d):
    f0 = cs[0]
    f1 = cs[1 % len(cs)]
    if shape == 0:
        have = f0(a, b)
        if not have:
            if c != 0 and f1(c, b):
                have = True
        return have
    if shape == 1:
        have = f0(a, b)
        if have:
            if d != 0:
                have = f1(d, b)
        return have
    if shape == 2:
        have = f0(a, b) or f1(c, b)
        if d != 0 and not have:
            have = True
        return have
    if shape == 3:
        have = False
        i = 0
        while i < d:
            if f0((a + i) % (1 << 64), b):
                have = True
            i += 1
        if not have:
            have = f1(c, b)
        return have
    x = f0(a, b)
    y = f1(c, b)
    have = False
    if x:
        have = True
    else:
        if y and d != 0:
            have = True
    return have


def gen_program(seed):
    r = random.Random(seed)
    ncal = r.randrange(1, 3)
    src, evs = [], []
    for i in range(ncal):
        nret = r.randrange(2, 5)
        text, guards, tail = gen_callee(r, f"small{i}", nret)
        src.append(text)
        evs.append((lambda g, t: (lambda k, cap: eval_callee(g, t, k, cap)))(guards, tail))
    shape = r.randrange(5)
    names = [(f"small{i}",) for i in range(ncal)]
    src.append(gen_outer(r, "outer", names, shape))

    main = ["fn main() -> i32 {"]
    n = 0
    for _ in range(r.randrange(6, 14)):
        a = r.randrange(0, 12)
        b = r.randrange(0, 12)
        c = r.randrange(0, 12)
        d = r.randrange(0, 4)
        want = eval_outer(shape, evs, a, b, c, d)
        n += 1
        if want:
            main.append(f"    if !outer({a}, {b}, {c}, {d}) {{ return {n} }}")
        else:
            main.append(f"    if outer({a}, {b}, {c}, {d}) {{ return {n} }}")
    main.append("    return 0")
    main.append("}")
    src.append("\n".join(main))
    head = (
        "// expect_exit: 0\n"
        f"// ROUND PHI -- generated bool/phi program, seed {seed}, shape {shape}.\n"
        "// Self checking: the expected answers were computed by the generator\n"
        "// over the same integers. Exit 0 is right; any other exit names the\n"
        "// assertion that broke.\n"
    )
    return head + "\n\n".join(src) + "\n"


def main():
    outdir, count = sys.argv[1], int(sys.argv[2])
    seed0 = int(sys.argv[3]) if len(sys.argv) > 3 else 0
    os.makedirs(outdir, exist_ok=True)
    for i in range(count):
        s = seed0 + i
        with open(os.path.join(outdir, f"g{s:05d}.fi"), "w") as fh:
            fh.write(gen_program(s))
    print(f"{count} programs in {outdir}")


if __name__ == "__main__":
    main()
