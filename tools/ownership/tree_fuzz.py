#!/usr/bin/env python3
import argparse
import os
import random
import shutil
import subprocess
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor

PRELUDE = """import std.io
import std.gcobj

gc class Node {
    left: Gc[Node],
    right: Gc[Node],
    v: u64,
}

gc class Holder {
    keep: Gc[Node],
    next: Gc[Holder],
}

struct Raw {
    a: Gc[Node],
}

fn leaf(v: u64) -> Gc[Node] {
    return gc Node { left: gc_null[Node](), right: gc_null[Node](), v: v } catch gc_null[Node]()
}

fn build(d: u32, v: u64) -> Gc[Node] {
    if d == 0 {
        return leaf(v)
    }
    let l: Gc[Node] = build(d - 1, v * 2)
    let r: Gc[Node] = build(d - 1, v * 2 + 1)
    return gc Node { left: l, right: r, v: v } catch gc_null[Node]()
}

fn sum_lim(t: Gc[Node], n: u32) -> u64 {
    if t == gc_null[Node]() || n == 0 {
        return 0
    }
    return t.v + sum_lim(t.left, n - 1) + sum_lim(t.right, n - 1)
}

fn holder() -> Gc[Holder] {
    return gc Holder { keep: gc_null[Node](), next: gc_null[Holder]() } catch gc_null[Holder]()
}

fn garbage(sink: Gc[Holder], n: u64) {
    var i: u64 = 0
    while i < n {
        let dead: Gc[Holder] = holder()
        sink.next = dead
        i = i + 1
    }
}
"""

NULL = "gc_null[Node]()"

HELPERS = {
    "keep_left": ("fn {n}(h: Gc[Holder], t: Gc[Node]) {{\n    if t != " + NULL + " {{\n        h.keep = t.left\n    }}\n}}\n", "void_ht"),
    "keep_right": ("fn {n}(h: Gc[Holder], t: Gc[Node]) {{\n    if t != " + NULL + " {{\n        h.keep = t.right\n    }}\n}}\n", "void_ht"),
    "keep_all": ("fn {n}(h: Gc[Holder], t: Gc[Node]) {{\n    h.keep = t\n}}\n", "void_ht"),
    "grab": ("fn {n}(t: Gc[Node]) -> Gc[Node] {{\n    if t == " + NULL + " {{\n        return t\n    }}\n    return t.left\n}}\n", "ret_t"),
    "deepest": ("fn {n}(t: Gc[Node]) -> Gc[Node] {{\n    var p: Gc[Node] = t\n    var k: u32 = 0\n    while p != " + NULL + " && p.right != " + NULL + " && k < 6 {{\n        p = p.right\n        k = k + 1\n    }}\n    return p\n}}\n", "ret_t"),
    "graft": ("fn {n}(t: Gc[Node], x: Gc[Node]) {{\n    if t != " + NULL + " {{\n        t.left = x\n    }}\n}}\n", "void_tt"),
    "swap": ("fn {n}(t: Gc[Node]) {{\n    if t != " + NULL + " {{\n        let x: Gc[Node] = t.left\n        t.left = t.right\n        t.right = x\n    }}\n}}\n", "void_t"),
    "link_back": ("fn {n}(t: Gc[Node]) {{\n    if t != " + NULL + " && t.left != " + NULL + " {{\n        t.left.right = t\n    }}\n}}\n", "void_t"),
    "steal": ("fn {n}(h: Gc[Holder], t: Gc[Node]) {{\n    if t != " + NULL + " {{\n        h.keep = t.right\n        t.right = " + NULL + "\n    }}\n}}\n", "void_ht"),
    "raw": ("fn {n}(t: Gc[Node], r: *mut u64) {{\n    if t != " + NULL + " {{\n        let p: *mut u64 = t as *mut u64\n        *r = *p\n    }}\n}}\n", "void_tr"),
    "push": ("fn {n}(vs: Gc[GcVec], t: Gc[Node]) {{\n    if t != " + NULL + " {{\n        let ok: bool = gcvec_append[Node](vs, t.right) catch false\n        if !ok {{\n            return\n        }}\n    }}\n}}\n", "void_vt"),
    "count": ("fn {n}(t: Gc[Node]) -> u64 {{\n    var p: Gc[Node] = t\n    var c: u64 = 0\n    var k: u32 = 0\n    while p != " + NULL + " && k < 9 {{\n        c = c + p.v % 7\n        p = p.left\n        k = k + 1\n    }}\n    return c\n}}\n", "ret_u"),
    "peek": ("fn {n}(t: Gc[Node]) -> u64 {{\n    if t == " + NULL + " || t.left == " + NULL + " {{\n        return 1\n    }}\n    return t.left.v % 11\n}}\n", "ret_u"),
    "wrap": ("fn {n}(t: Gc[Node]) -> Gc[Node] {{\n    return gc Node {{ left: t, right: " + NULL + ", v: 5 }} catch " + NULL + "\n}}\n", "ret_t"),
}

TREES = 4
HOLDERS = 2


class Gen:
    def __init__(self, seed):
        self.r = random.Random(seed)
        self.helpers = []

    def tree(self):
        return f"t{self.r.randrange(TREES)}"

    def hold(self):
        return f"h{self.r.randrange(HOLDERS)}"

    def stmt(self, depth):
        r = self.r
        c = r.randrange(14)
        if c <= 2:
            return [f"{self.tree()} = build({r.randrange(5)}, i % 97 + {r.randrange(50)})"]
        if c == 3:
            return [f"{self.tree()} = {self.tree()}"]
        if c == 4:
            a, b = self.tree(), self.tree()
            side = r.choice(["left", "right"])
            return [f"if {b} != {NULL} {{", f"    {a} = {b}.{side}", "}"]
        if c == 5:
            return [f"total = total + sum_lim({self.tree()}, 7)"]
        if c == 6 and self.helpers:
            name, kind = r.choice(self.helpers)
            return self.call_on(name, kind, self.tree())
        if c == 7:
            return [f"total = total + sum_lim({self.hold()}.keep, 7)"]
        if c == 8:
            return [f"garbage({self.hold()}, {r.randrange(1, 40)})"]
        if c == 9:
            return [f"if i % {r.randrange(3, 30)} == 0 {{", "    gc_collect()", "}"]
        if c == 10:
            n = f"x{r.randrange(1000000)}"
            out = [f"let {n}: Gc[Node] = build({r.randrange(4)}, i)"]
            if self.helpers and r.random() < 0.5:
                name, kind = r.choice(self.helpers)
                out += self.call_on(name, kind, n)
            out.append(f"total = total + sum_lim({n}, 7)")
            return out
        if c == 11:
            return [f"total = total + sum_lim(build({r.randrange(4)}, i + {r.randrange(9)}), 7)"]
        if c == 12:
            return [f"total = total + sum_lim((*rr).a, 7) + gcvec_length(vs)"]
        if depth < 2:
            body = []
            for _ in range(r.randrange(1, 4)):
                body += self.stmt(depth + 1)
            return [f"if i % {r.randrange(2, 5)} == {r.randrange(2)} {{"] + ["    " + s for s in body] + ["}"]
        return [f"{self.hold()}.keep = {self.tree()}"]

    def call_on(self, name, kind, n):
        if kind == "void_ht":
            return [f"{name}({self.hold()}, {n})"]
        if kind == "void_tt":
            return [f"{name}({n}, {self.tree()})" if self.r.random() < 0.5 else f"{name}({self.tree()}, {n})"]
        if kind == "void_t":
            return [f"{name}({n})"]
        if kind == "void_tr":
            return [f"{name}({n}, rp)"]
        if kind == "void_vt":
            return [f"{name}(vs, {n})"]
        if kind == "ret_u":
            return [f"total = total + {name}({n})"]
        if self.r.random() < 0.5:
            return [f"total = total + sum_lim({name}({n}), 6)"]
        return [f"{self.tree()} = {name}({n})"]

    def program(self):
        r = self.r
        picks = r.sample(sorted(HELPERS), r.randrange(2, 7))
        src = [PRELUDE]
        for k in picks:
            text, kind = HELPERS[k]
            name = f"h_{k}"
            attr = "#[no_inline]\n" if r.random() < 0.6 else ""
            src.append(attr + text.format(n=name))
            self.helpers.append((name, kind))
        body = []
        for _ in range(r.randrange(8, 30)):
            body += self.stmt(0)
        iters = r.randrange(20, 120)
        main = ["fn main() -> i32 {", "    if !gc_init() {", "        return 3", "    }"]
        for k in range(HOLDERS):
            main.append(f"    let h{k}: Gc[Holder] = holder()")
        main += [
            "    let vs: Gc[GcVec] = gcvec_new() catch gc_null[GcVec]()",
            "    let rp: *mut u64 = gcobj.alloc[Raw](size_of[Raw]()) as *mut u64",
            "    let rr: *mut Raw = rp as *mut Raw",
            "    if rp as u64 == 0 {",
            "        return 4",
            "    }",
        ]
        for k in range(TREES):
            main.append(f"    var t{k}: Gc[Node] = build({r.randrange(4)}, {k})")
        main += ["    var total: u64 = 0", "    var i: u64 = 0", f"    while i < {iters} {{"]
        main += ["        " + s for s in body]
        main += ["        i = i + 1", "    }", "    gc_collect()"]
        tail = " + ".join([f"sum_lim(t{k}, 7)" for k in range(TREES)] + [f"sum_lim(h{k}.keep, 7)" for k in range(HOLDERS)])
        main.append(f"    total = total + {tail} + sum_lim((*rr).a, 7)")
        main.append("    var k: u64 = 0")
        main.append("    while k < gcvec_length(vs) {")
        main.append("        total = total + sum_lim(gcvec_read[Node](vs, k), 7)")
        main.append("        k = k + 1")
        main.append("    }")
        main.append('    io.fmt_print_line(f"total {total}")')
        main += ["    return 0", "}"]
        src.append("\n".join(main) + "\n")
        return "\n".join(src)


VARIANTS = [("off", {}), ("strict", {}), ("strict", {"FIRN_OWN_NO_AST": "1"}), ("on", {"FIRN_OWN_NO_AST": "1"})]


def run_one(args, seed):
    src = Gen(seed).program()
    d = tempfile.mkdtemp(prefix="treefuzz.")
    try:
        p = os.path.join(d, "p.fi")
        with open(p, "w") as fh:
            fh.write(src)
        results = []
        trees = 0
        for mode, extra in VARIANTS:
            env = dict(os.environ, FIRNLIB=args.lib, FIRN_OWN_TRACE="1", **extra)
            exe = os.path.join(d, f"b_{mode}_{len(results)}")
            c = subprocess.run([args.firnc, "--no-cache", f"--ownership={mode}", p, "-o", exe], env=env, capture_output=True, text=True, timeout=300)
            if c.returncode != 0:
                return seed, "build", f"{mode} {extra}: {c.stderr[:400]}", src, 0
            trees += sum(1 for l in c.stderr.splitlines() if " tree drops " in l and not l.startswith("ownership fir: build "))
            try:
                x = subprocess.run([exe], capture_output=True, text=True, timeout=60, cwd=d)
                results.append((x.returncode, x.stdout))
            except subprocess.TimeoutExpired:
                results.append(("timeout", ""))
        ref = results[0]
        for (mode, extra), res in zip(VARIANTS[1:], results[1:]):
            if res != ref:
                return seed, "diff", f"{mode} {extra}: rc {res[0]} vs off {ref[0]}; {res[1].strip()[:80]} vs {ref[1].strip()[:80]}", src, trees
        if ref[0] != 0:
            return seed, "offrc", f"off rc {ref[0]}", src, trees
        return seed, "ok", "", "", trees
    finally:
        shutil.rmtree(d, ignore_errors=True)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--firnc", required=True)
    ap.add_argument("--lib", required=True)
    ap.add_argument("--count", type=int, default=2000)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--out", required=True)
    ap.add_argument("--emit", type=int, default=-1)
    args = ap.parse_args()
    if args.emit >= 0:
        sys.stdout.write(Gen(args.emit).program())
        return 0
    os.makedirs(args.out, exist_ok=True)
    counts = {}
    with_trees = 0
    seeds = range(args.seed, args.seed + args.count)
    with ThreadPoolExecutor(args.jobs) as ex:
        for seed, kind, msg, src, trees in ex.map(lambda s: run_one(args, s), seeds):
            counts[kind] = counts.get(kind, 0) + 1
            with_trees += trees > 0
            if kind != "ok":
                with open(os.path.join(args.out, f"{kind}_{seed}.fi"), "w") as fh:
                    fh.write(src)
                print(f"seed {seed} {kind}: {msg}", flush=True)
    print(f"tree_fuzz: {args.count} programs, {with_trees} with tree drops, " + ", ".join(f"{k} {v}" for k, v in sorted(counts.items())), flush=True)
    return 0 if set(counts) <= {"ok"} else 1


if __name__ == "__main__":
    sys.exit(main())
