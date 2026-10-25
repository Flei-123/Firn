#!/usr/bin/env python3
# tools/diff_c0c1_gen.py -- deterministic generator of the firnc0-vs-firnc1 differential corpus.
#
# Writes tests/diff/*.fi (fixed seed, byte-identical on every run). The corpus is committed;
# regenerate with:  python3 tools/diff_c0c1_gen.py tests/diff
#
# Header lines understood by tools/diff_c0c1.sh:
#   // diff: run | neg          run = build with both compilers at every level and run;
#                               neg = both compilers must reject, same error class
#   // levels: same            output must also be equal between ALL opt levels
#   // levels: fast_differs    release-fast is allowed to differ (unchecked wraparound)
import os
import random
import sys

SEED = 4800
out_dir = sys.argv[1] if len(sys.argv) > 1 else "tests/diff"
rng = random.Random(SEED)

PRELUDE = """fn putu(v: u64) {
    var buf: [u8; 24] = [0; 24]
    var u: u64 = v
    var i: usize = 23
    buf[23] = 10
    if u == 0 {
        i = 22
        buf[22] = 48
    }
    while u > 0 {
        i = i - 1
        buf[i] = (48 + u % 10) as u8
        u = u / 10
    }
    syscall(1, 1, &buf[i], (24 - i) as u64)
}
fn put(v: i64) {
    if v < 0 {
        var m: [u8; 1] = [45]
        syscall(1, 1, &m[0], 1)
        putu(0 -% (v as u64))
        return
    }
    putu(v as u64)
}
"""

BITS = {"i8": 8, "i16": 16, "i32": 32, "i64": 64, "u8": 8, "u16": 16, "u32": 32, "u64": 64}
SIGNED = ["i8", "i16", "i32", "i64"]
UNSIGNED = ["u8", "u16", "u32", "u64"]
ALLT = SIGNED + UNSIGNED

files = {}


def emit(name, kind, levels, body, prelude=True):
    head = "// diff: %s\n" % kind
    if kind == "run":
        head += "// levels: %s\n" % levels
    src = head + (PRELUDE if prelude and kind == "run" else "") + body
    assert name not in files, name
    files[name] = src


def tmin(t):
    return -(1 << (BITS[t] - 1)) if t[0] == "i" else 0


def tmax(t):
    return (1 << (BITS[t] - 1)) - 1 if t[0] == "i" else (1 << BITS[t]) - 1


def lit(t, v):
    if v < 0 and v == tmin(t):
        return "(%d - 1)" % (v + 1)
    if v < 0:
        return "(0 - %d)" % (-v)
    return str(v)


def fits(t, v):
    return tmin(t) <= v <= tmax(t)


def shown(t, expr):
    if t == "u64":
        return "putu(%s)" % expr
    return "put(%s as i64)" % expr


# ---------------------------------------------------------------- overflow
def ovf_prog(t, op, a, b, opaque):
    if opaque:
        decl = "    var arr: [%s; 2] = [%s, %s]\n    let a: %s = arr[0]\n    let b: %s = arr[1]\n" % (
            t, lit(t, a), lit(t, b), t, t)
    else:
        decl = "    let a: %s = %s\n    let b: %s = %s\n" % (t, lit(t, a), t, lit(t, b))
    return ("fn main() -> i32 {\n" + decl + "    let c: %s = a %s b\n    %s\n    return 0\n}\n" %
            (t, op, shown(t, "c")))


for t in ALLT:
    lo, hi = tmin(t), tmax(t)
    cases = {
        "+": [(hi, 1, True), (hi - 1, 1, False)],
        "-": [(lo, 1, True), (lo + 1, 1, False)],
        "*": [(hi, 2, True), (hi // 2, 2, False)],
    }
    opn = {"+": "add", "-": "sub", "*": "mul"}
    for op, cs in cases.items():
        for i, (a, b, overflow) in enumerate(cs):
            for opaque in (False, True):
                if i == 1 and not opaque:
                    continue
                res = {"+": a + b, "-": a - b, "*": a * b}[op]
                lev = "fast_differs" if not fits(t, res) else "same"
                emit("ovf_%s_%s_%d%s" % (opn[op], t, i, "_arr" if opaque else "_lit"), "run", lev,
                     ovf_prog(t, op, a, b, opaque))

for t in ALLT:
    for op, nm in (("+%", "wadd"), ("-%", "wsub"), ("*%", "wmul")):
        hi, lo = tmax(t), tmin(t)
        a, b = {"+%": (hi, 3), "-%": (lo, 5), "*%": (hi, hi)}[op]
        emit("wrap_%s_%s" % (nm, t), "run", "same",
             "fn main() -> i32 {\n    var arr: [%s; 2] = [%s, %s]\n    let c: %s = arr[0] %s arr[1]\n    %s\n"
             "    let d: %s = %s %s %s\n    %s\n    return 0\n}\n" %
             (t, lit(t, a), lit(t, b), t, op, shown(t, "c"), t, lit(t, a), op, lit(t, b), shown(t, "d")))

# ---------------------------------------------------------------- casts
cast_vals = [0, 1, 127, 128, 255, 256, 32767, 32768, 65535, 65536, 2147483647, 2147483648,
             4294967295, 4294967296, -1, -128, -129, -32768, -32769, -2147483648, -2147483649,
             9223372036854775807, 18446744073709551615, -9223372036854775808]
cast_pairs = []
for src in ["u64", "i64", "u32", "i32", "u16", "i16"]:
    for dst in ["u8", "u16", "u32", "i8", "i16", "i32", "i64", "u64"]:
        if src != dst:
            cast_pairs.append((src, dst))
rng.shuffle(cast_pairs)
n_cast = 0
for src, dst in cast_pairs:
    cand = [v for v in cast_vals if fits(src, v)]
    for v in rng.sample(cand, 2):
        lev = "same" if fits(dst, v) else "fast_differs"
        emit("cast_%s_%s_%03d" % (src, dst, n_cast), "run", lev,
             "fn main() -> i32 {\n    var arr: [%s; 1] = [%s]\n    let c: %s = arr[0] as %s\n    %s\n"
             "    let k: %s = %s\n    let e: %s = k as %s\n    %s\n    return 0\n}\n" %
             (src, lit(src, v), dst, dst, shown(dst, "c"), src, lit(src, v), dst, dst, shown(dst, "e")))
        n_cast += 1
        if n_cast >= 60:
            break
    if n_cast >= 60:
        break

# ---------------------------------------------------------------- division
for t in ALLT:
    for op, nm in (("/", "div"), ("%", "rem")):
        emit("div_%s0_%s" % (nm, t), "run", "fast_differs",
             "fn main() -> i32 {\n    var arr: [%s; 2] = [%s, 0]\n    let c: %s = arr[0] %s arr[1]\n    %s\n    return 0\n}\n" %
             (t, lit(t, 100 if t[0] == "u" else -100), t, op, shown(t, "c")))
        emit("div_%s_ok_%s" % (nm, t), "run", "same",
             "fn main() -> i32 {\n    var arr: [%s; 2] = [%s, 7]\n    let c: %s = arr[0] %s arr[1]\n    %s\n    return 0\n}\n" %
             (t, lit(t, 100 if t[0] == "u" else -100), t, op, shown(t, "c")))
for t in SIGNED:
    for op, nm in (("/", "div"), ("%", "rem")):
        emit("div_%s_min_m1_%s" % (nm, t), "run", "fast_differs",
             "fn main() -> i32 {\n    var arr: [%s; 2] = [%s, %s]\n    let c: %s = arr[0] %s arr[1]\n    %s\n    return 0\n}\n" %
             (t, lit(t, tmin(t)), lit(t, -1), t, op, shown(t, "c")))

# ---------------------------------------------------------------- shifts
for t in ALLT:
    w = BITS[t]
    for op, nm in (("<<", "shl"), (">>", "shr")):
        for amt in sorted({w - 1, w}):
            ok = amt < w
            emit("shift_%s_%s_%d" % (nm, t, amt), "run", "same" if ok else "fast_differs",
                 "fn main() -> i32 {\n    var arr: [%s; 2] = [%s, %d]\n    let c: %s = arr[0] %s arr[1]\n    %s\n"
                 "    let d: %s = %s %s %d\n    %s\n    return 0\n}\n" %
                 (t, lit(t, 1 if t[0] == "u" else -1) if op == ">>" else lit(t, 3), amt, t, op,
                  shown(t, "c"), t, lit(t, 1 if t[0] == "u" else -1) if op == ">>" else lit(t, 3), op, amt,
                  shown(t, "d")))

# ---------------------------------------------------------------- layout
FT = ["u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "bool"]
for n in range(30):
    nf = rng.randint(2, 6)
    fields = [rng.choice(FT) for _ in range(nf)]
    decl = "struct S {\n" + "".join("    f%d: %s,\n" % (i, ft) for i, ft in enumerate(fields)) + "}\n"
    init = ", ".join("f%d: %s" % (i, "true" if ft == "bool" else "%d" % (i + 1)) for i, ft in enumerate(fields))
    body = decl
    nested = n % 5 == 4
    if nested:
        body += "struct W {\n    p: u8,\n    s: S,\n    q: u16,\n}\n"
    body += "fn main() -> i32 {\n"
    body += "    var s: S = S { %s, }\n" % init
    body += "    let base: u64 = (&s.f0) as u64\n"
    for i in range(1, nf):
        body += "    putu((&s.f%d) as u64 - base)\n" % i
    body += "    putu(size_of[S]() as u64)\n"
    body += "    var arr: [S; 2] = [s, s]\n"
    body += "    putu((&arr[1].f0) as u64 - (&arr[0].f0) as u64)\n"
    if nested:
        body += "    var w: W = W { p: 1, s: s, q: 2, }\n"
        body += "    let wb: u64 = (&w.p) as u64\n"
        body += "    putu((&w.s.f0) as u64 - wb)\n    putu((&w.q) as u64 - wb)\n    putu(size_of[W]() as u64)\n"
    body += "    return 0\n}\n"
    emit("layout_%02d" % n, "run", "same", body)

for t in ["u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "bool"]:
    emit("layout_sizeof_%s" % t, "run", "same",
         "struct A { a: %s }\nstruct B { a: %s, b: %s }\nstruct C { a: u8, b: %s }\nstruct D { a: %s, b: u8 }\n"
         "fn main() -> i32 {\n    putu(size_of[%s]() as u64)\n    putu(size_of[A]() as u64)\n    putu(size_of[B]() as u64)\n"
         "    putu(size_of[C]() as u64)\n    putu(size_of[D]() as u64)\n    return 0\n}\n" % (t, t, t, t, t, t))
emit("layout_attr_packed", "run", "same",
     "#[packed]\nstruct P {\n    a: u8,\n    b: u32,\n    c: u16,\n    d: u64,\n}\n"
     "#[packed]\nstruct Q {\n    a: u8,\n    f: f64,\n    g: f32,\n}\n"
     "struct W {\n    x: u8,\n    p: P,\n    y: u8,\n}\n"
     "fn total(p: P) -> u64 {\n    return (p.a as u64) + (p.b as u64) + (p.c as u64) + p.d\n}\n"
     "fn main() -> i32 {\n    var p: P = P { a: 1, b: 2, c: 3, d: 4 }\n    let base: u64 = (&p.a) as u64\n"
     "    putu(size_of[P]() as u64)\n    putu(size_of[Q]() as u64)\n    putu(size_of[W]() as u64)\n"
     "    putu((&p.b) as u64 - base)\n    putu((&p.c) as u64 - base)\n    putu((&p.d) as u64 - base)\n"
     "    p.b = 0x01020304\n    p.d = 0x0102030405060708\n    p.c = 0xABCD\n"
     "    putu(p.a as u64)\n    putu(p.b as u64)\n    putu(p.c as u64)\n    putu(p.d)\n"
     "    var arr: [P; 2] = [p, p]\n    arr[1].d = 99\n"
     "    putu((&arr[1].a) as u64 - (&arr[0].a) as u64)\n    putu(arr[0].d)\n    putu(arr[1].d)\n"
     "    putu(total(p))\n"
     "    var q: Q = Q { a: 7, f: 2.5, g: 4.0 }\n    q.f = 1.5\n"
     "    putu(q.a as u64)\n    putu((q.f * 2.0) as u64)\n    putu((q.g * 2.0) as u64)\n"
     "    var w: W = W { x: 5, p: p, y: 6 }\n    let wb: u64 = (&w.x) as u64\n"
     "    putu((&w.p) as u64 - wb)\n    putu((&w.y) as u64 - wb)\n    putu(w.p.d)\n    return 0\n}\n")
emit("layout_attr_align", "run", "same",
     "#[align(16)]\nstruct A {\n    a: u8,\n}\n"
     "#[align(64)]\nstruct B {\n    a: u8,\n    b: u32,\n}\n"
     "#[packed]\n#[align(8)]\nstruct D {\n    a: u8,\n    b: u32,\n}\n"
     "struct W {\n    x: u8,\n    b: B,\n    y: u8,\n}\n"
     "fn first(b: B) -> u8 {\n    return b.a\n}\n"
     "fn main() -> i32 {\n    putu(size_of[A]() as u64)\n    putu(size_of[B]() as u64)\n    putu(size_of[D]() as u64)\n    putu(size_of[W]() as u64)\n"
     "    var b: B = B { a: 9, b: 8 }\n    putu((&b) as u64 % 64)\n"
     "    var arr: [B; 3] = [b, b, b]\n    putu((&arr[0]) as u64 % 64)\n    putu((&arr[2]) as u64 % 64)\n"
     "    putu((&arr[2]) as u64 - (&arr[0]) as u64)\n"
     "    var w: W = W { x: 1, b: b, y: 2 }\n    putu((&w) as u64 % 64)\n    putu((&w.b) as u64 - (&w) as u64)\n"
     "    putu((&w.y) as u64 - (&w) as u64)\n    putu(first(b) as u64)\n"
     "    var d: D = D { a: 1, b: 2 }\n    putu((&d.b) as u64 - (&d.a) as u64)\n    putu(d.b as u64)\n    return 0\n}\n")
for bad in ("3", "0", "8192"):
    emit("layout_attr_align_bad_%s" % bad, "neg", "",
         "#[align(%s)]\nstruct S { a: u8, b: u32 }\nfn main() -> i32 {\n    return 0\n}\n" % bad)

# ---------------------------------------------------------------- ABI
def abi_args(n):
    ts = [rng.choice(["i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "bool"]) for _ in range(n)]
    ps = ", ".join("a%d: %s" % (i, t) for i, t in enumerate(ts))
    terms = []
    for i, t in enumerate(ts):
        if t == "bool":
            terms.append("(if a%d { 1 } else { 0 })" % i if False else "(a%d as u8) as i64" % i)
        else:
            terms.append("a%d as i64" % i)
    s = "    var acc: i64 = 0\n"
    for i, tm in enumerate(terms):
        s += "    acc = acc *%% 31 +%% (%s)\n" % tm
    s += "    return acc\n"
    vals = []
    for i, t in enumerate(ts):
        if t == "bool":
            vals.append(rng.choice(["true", "false"]))
        elif t[0] == "i":
            vals.append(lit(t, rng.choice([tmin(t), -1, 0, 1, tmax(t), rng.randint(tmin(t), tmax(t))])))
        else:
            vals.append(lit(t, rng.choice([0, 1, tmax(t), tmax(t) // 2 + 1, rng.randint(0, tmax(t))])))
    return ps, s, vals


for n in range(1, 15):
    ps, s, vals = abi_args(n)
    body = "fn f(%s) -> i64 {\n%s}\n" % (ps, s)
    body += "fn main() -> i32 {\n    put(f(%s))\n    return 0\n}\n" % ", ".join(vals)
    emit("abi_args_%02d" % n, "run", "same", body)

STRUCTS = [
    ("T1", ["u8"]), ("T2", ["u8", "u8"]), ("T3", ["u8", "u16"]), ("T4", ["u32"]), ("T6", ["u16", "u32"]),
    ("T8", ["u64"]), ("T12", ["u32", "u32", "u32"]), ("T16", ["u64", "u64"]), ("T17", ["u64", "u64", "u8"]),
    ("T24", ["i64", "i64", "i64"]), ("T32", ["i64", "i64", "i64", "i64"]), ("T40", ["i64", "u32", "i64", "u8", "i64", "i16"]),
    ("T9", ["u8", "u64"]), ("T5", ["u8", "u8", "u8", "u8", "u8"]),
]
for nm, fl in STRUCTS:
    decl = "struct %s {\n" % nm + "".join("    f%d: %s,\n" % (i, ft) for i, ft in enumerate(fl)) + "}\n"
    mk = ", ".join("f%d: %d" % (i, rng.randint(1, 100)) for i in range(len(fl)))
    summ = " + ".join("(s.f%d as i64)" % i for i in range(len(fl)))
    body = decl
    body += "fn make(k: i64) -> %s {\n    return %s { %s, }\n}\n" % (nm, nm, ", ".join("f%d: (k + %d) as %s" % (i, i, ft) for i, ft in enumerate(fl)))
    body += "fn total(s: %s) -> i64 {\n    return %s\n}\n" % (nm, summ)
    body += "fn bump(s: %s) -> %s {\n    var r: %s = s\n    r.f0 = r.f0 + 1\n    return r\n}\n" % (nm, nm, nm)
    body += "fn main() -> i32 {\n    let a: %s = make(5)\n    let b: %s = bump(a)\n    put(total(a))\n    put(total(b))\n" % (nm, nm)
    body += "    put(total(bump(bump(make(9)))))\n    let c: %s = %s { %s, }\n    put(total(c))\n    return 0\n}\n" % (nm, nm, mk)
    emit("abi_struct_%s" % nm, "run", "same", body)

for k in range(4):
    nm, fl = STRUCTS[7 + k]
    decl = "struct Q { a: i64, b: i64 }\nstruct Big { a: i64, b: i64, c: i64, d: i64 }\n"
    body = decl + "fn mix(x: i32, q: Q, y: i32, g: Big, z: i32, w: i32, v: u8, u: i16) -> i64 {\n"
    body += "    return q.a + q.b * 3 + g.a + g.b * 5 + g.c + g.d * 7 + (x + y + z + w) as i64 + (v as i64) * 11 + (u as i64)\n}\n"
    body += "fn main() -> i32 {\n    let q: Q = Q { a: %d, b: %d }\n    let g: Big = Big { a: %d, b: %d, c: %d, d: %d }\n" % tuple(
        rng.randint(-50, 50) for _ in range(6))
    body += "    put(mix(%d, q, %d, g, %d, %d, %d, %d))\n    return 0\n}\n" % (
        rng.randint(-9, 9), rng.randint(-9, 9), rng.randint(-9, 9), rng.randint(-9, 9), rng.choice([0, 255, 7]), rng.choice([-32768, 32767, -1]))
    emit("abi_mixed_%d" % k, "run", "same", body)

emit("abi_callee_saved", "run", "same",
     "fn g(x: i64) -> i64 { return x * 3 + 1 }\nfn f(a: i64, b: i64, c: i64, d: i64, e: i64) -> i64 {\n"
     "    let t1: i64 = g(a)\n    let t2: i64 = g(b + t1)\n    let t3: i64 = g(c + t2)\n    let t4: i64 = g(d + t3)\n"
     "    return a + b + c + d + e + t1 + t2 + t3 + t4 + g(e)\n}\nfn main() -> i32 {\n    put(f(1, 2, 3, 4, 5))\n    put(f(-7, 80, -900, 1000, -11))\n    return 0\n}\n")
emit("abi_recursion_ack", "run", "same",
     "fn ack(m: i64, n: i64) -> i64 {\n    if m == 0 { return n + 1 }\n    if n == 0 { return ack(m - 1, 1) }\n    return ack(m - 1, ack(m, n - 1))\n}\n"
     "fn main() -> i32 {\n    put(ack(2, 3))\n    put(ack(3, 3))\n    return 0\n}\n")
emit("abi_mutual_rec", "run", "same",
     "fn ev(n: u32) -> bool {\n    if n == 0 { return true }\n    return od(n - 1)\n}\nfn od(n: u32) -> bool {\n    if n == 0 { return false }\n    return ev(n - 1)\n}\n"
     "fn main() -> i32 {\n    if ev(100) { put(1) } else { put(0) }\n    if ev(37) { put(1) } else { put(0) }\n    return 0\n}\n")
emit("abi_narrow_ret", "run", "same",
     "fn r8(x: i64) -> i8 { return x as i8 }\nfn ru8(x: u64) -> u8 { return (x & 255) as u8 }\nfn r16(x: i64) -> i16 { return (x & 32767) as i16 }\n"
     "fn rb(x: i64) -> bool { return x > 3 }\nfn main() -> i32 {\n    put(r8(-100) as i64)\n    put(ru8(1023) as i64)\n    put(r16(70000) as i64)\n"
     "    if rb(5) { put(1) } else { put(0) }\n    return 0\n}\n")
emit("abi_ptr_args", "run", "same",
     "fn fill(p: *mut u8, n: usize, v: u8) {\n    var i: usize = 0\n    while i < n {\n        p[i] = v + (i as u8)\n        i = i + 1\n    }\n}\n"
     "fn main() -> i32 {\n    var b: [u8; 8] = [0; 8]\n    fill(&b[0], 8, 10)\n    var k: usize = 0\n    while k < 8 {\n        put(b[k] as i64)\n        k = k + 1\n    }\n    return 0\n}\n")
emit("abi_exit_code", "run", "same", "fn main() -> i32 {\n    put(1)\n    return 300\n}\n")
emit("abi_exit_neg", "run", "same", "fn main() -> i32 {\n    put(1)\n    return 0 - 3\n}\n")
emit("abi_ten_args_stack", "run", "same",
     "fn f(a: i64, b: i64, c: i64, d: i64, e: i64, f: i64, g: i64, h: i64, i: i64, j: i64) -> i64 {\n    return a - b + c * d - e + f * g - h + i * j\n}\n"
     "fn main() -> i32 {\n    put(f(1, 2, 3, 4, 5, 6, 7, 8, 9, 10))\n    put(f(-1, 2, -3, 4, -5, 6, -7, 8, -9, 10))\n    return 0\n}\n")

# ---------------------------------------------------------------- opt (algorithmic)
def tpl(t, **kw):
    for k, v in kw.items():
        t = t.replace("@%s@" % k, str(v))
    return t


T_SIEVE = """fn main() -> i32 {
    var s: [u8; @N@] = [1; @N@]
    s[0] = 0
    s[1] = 0
    var i: usize = 2
    while i * i < @N@ {
        if s[i] == 1 {
            var j: usize = i * i
            while j < @N@ {
                s[j] = 0
                j = j + i
            }
        }
        i = i + 1
    }
    var c: u64 = 0
    var k: usize = 0
    while k < @N@ {
        c = c + (s[k] as u64)
        k = k + 1
    }
    putu(c)
    return 0
}
"""
T_COLLATZ = """fn steps(n: u64) -> u64 {
    var x: u64 = n
    var c: u64 = 0
    while x != 1 {
        if x % 2 == 0 { x = x / 2 } else { x = 3 * x + 1 }
        c = c + 1
    }
    return c
}
fn main() -> i32 {
    var best: u64 = 0
    var arg: u64 = 0
    var n: u64 = @LO@
    while n < @HI@ {
        let s: u64 = steps(n)
        if s > best {
            best = s
            arg = n
        }
        n = n + 1
    }
    putu(arg)
    putu(best)
    return 0
}
"""
T_GCD = """fn gcd(a: u64, b: u64) -> u64 {
    var x: u64 = a
    var y: u64 = b
    while y != 0 {
        let t: u64 = x % y
        x = y
        y = t
    }
    return x
}
fn main() -> i32 {
    var acc: u64 = 0
    var i: u64 = 1
    while i < @N@ {
        acc = acc + gcd(i * @A@, @B@ + i)
        i = i + 1
    }
    putu(acc)
    return 0
}
"""
T_LCG = """fn main() -> i32 {
    var x: u64 = @SEED@
    var h: u64 = 14695981039346656037
    var i: u64 = 0
    while i < @N@ {
        x = x *% 6364136223846793005 +% 1442695040888963407
        h = (h ^ (x >> 33)) *% 1099511628211
        i = i + 1
    }
    putu(x)
    putu(h)
    return 0
}
"""
T_SORT = """fn main() -> i32 {
    var a: [i64; @N@] = [0; @N@]
    var x: u64 = @SEED@
    var i: usize = 0
    while i < @N@ {
        x = x *% 6364136223846793005 +% 1442695040888963407
        a[i] = ((x >> 40) as i64) - 4000000
        i = i + 1
    }
    var n: usize = @N@
    while n > 1 {
        var j: usize = 0
        while j + 1 < n {
            if a[j] > a[j + 1] {
                let t: i64 = a[j]
                a[j] = a[j + 1]
                a[j + 1] = t
            }
            j = j + 1
        }
        n = n - 1
    }
    put(a[0])
    put(a[@N@ / 2])
    put(a[@N@ - 1])
    var sum: i64 = 0
    var k: usize = 0
    while k < @N@ {
        sum = sum + a[k] * ((k as i64) + 1)
        k = k + 1
    }
    put(sum)
    return 0
}
"""
T_MAT = """fn main() -> i32 {
    var a: [i64; 16] = [0; 16]
    var b: [i64; 16] = [0; 16]
    var c: [i64; 16] = [0; 16]
    var i: usize = 0
    while i < 16 {
        a[i] = ((i * @A@ + 3) % 17) as i64 - 8
        b[i] = ((i * @B@ + 5) % 13) as i64 - 6
        i = i + 1
    }
    var r: usize = 0
    while r < 4 {
        var q: usize = 0
        while q < 4 {
            var acc: i64 = 0
            var k: usize = 0
            while k < 4 {
                acc = acc + a[r * 4 + k] * b[k * 4 + q]
                k = k + 1
            }
            c[r * 4 + q] = acc
            q = q + 1
        }
        r = r + 1
    }
    var t: i64 = 0
    var z: usize = 0
    while z < 16 {
        t = t * 7 + c[z]
        z = z + 1
    }
    put(t)
    put(c[5])
    return 0
}
"""
T_BITS = """fn popcnt(v: u64) -> u64 {
    var x: u64 = v
    var c: u64 = 0
    while x != 0 {
        x = x & (x - 1)
        c = c + 1
    }
    return c
}
fn rev8(v: u8) -> u8 {
    var r: u8 = 0
    var x: u8 = v
    var i: u32 = 0
    while i < 8 {
        r = (r << 1) | (x & 1)
        x = x >> 1
        i = i + 1
    }
    return r
}
fn main() -> i32 {
    var s: u64 = 0
    var x: u64 = @SEED@
    var i: u64 = 0
    while i < @N@ {
        x = x *% 6364136223846793005 +% 1442695040888963407
        s = s + popcnt(x) + (rev8((x >> 56) as u8) as u64)
        i = i + 1
    }
    putu(s)
    return 0
}
"""
T_FIB = """fn main() -> i32 {
    var a: u64 = @A@
    var b: u64 = @B@
    var i: u32 = 0
    while i < @N@ {
        let t: u64 = a +% b
        a = b
        b = t
        i = i + 1
    }
    putu(a)
    putu(b)
    return 0
}
"""
T_BSEARCH = """fn main() -> i32 {
    var a: [i32; @N@] = [0; @N@]
    var i: usize = 0
    while i < @N@ {
        a[i] = (i as i32) * @STEP@ - 100
        i = i + 1
    }
    var found: i64 = 0
    var t: i32 = -150
    while t < @N@ * @STEP@ {
        var lo: usize = 0
        var hi: usize = @N@
        while lo < hi {
            let mid: usize = lo + (hi - lo) / 2
            if a[mid] < t { lo = mid + 1 } else { hi = mid }
        }
        if lo < @N@ {
            if a[lo] == t { found = found + 1 }
        }
        t = t + 1
    }
    put(found)
    return 0
}
"""
T_LCS = """fn main() -> i32 {
    var s1: [u8; @N@] = [0; @N@]
    var s2: [u8; @N@] = [0; @N@]
    var x: u64 = @SEED@
    var i: usize = 0
    while i < @N@ {
        x = x *% 6364136223846793005 +% 1442695040888963407
        s1[i] = ((x >> 50) % 4) as u8
        x = x *% 6364136223846793005 +% 1442695040888963407
        s2[i] = ((x >> 50) % 4) as u8
        i = i + 1
    }
    var dp: [u32; (@N@ + 1) * (@N@ + 1)] = [0; (@N@ + 1) * (@N@ + 1)]
    var r: usize = 1
    while r <= @N@ {
        var q: usize = 1
        while q <= @N@ {
            let w: usize = @N@ + 1
            if s1[r - 1] == s2[q - 1] {
                dp[r * w + q] = dp[(r - 1) * w + q - 1] + 1
            } else {
                let up: u32 = dp[(r - 1) * w + q]
                let lf: u32 = dp[r * w + q - 1]
                if up > lf { dp[r * w + q] = up } else { dp[r * w + q] = lf }
            }
            q = q + 1
        }
        r = r + 1
    }
    putu(dp[@N@ * (@N@ + 1) + @N@] as u64)
    return 0
}
"""
T_BRANCH = """fn classify(v: i64) -> i64 {
    if v < -50 { return -2 }
    if v < 0 { return -1 }
    if v == 0 { return 0 }
    if v < 50 { return 1 }
    if v < 1000 { return 2 }
    return 3
}
fn main() -> i32 {
    var x: u64 = @SEED@
    var hist: [i64; 6] = [0; 6]
    var i: u64 = 0
    while i < @N@ {
        x = x *% 6364136223846793005 +% 1442695040888963407
        let v: i64 = ((x >> 33) % 2200) as i64 - 300
        let c: i64 = classify(v)
        hist[(c + 2) as usize] = hist[(c + 2) as usize] + 1
        if c == 3 && v % 3 == 0 { hist[5] = hist[5] + 1 }
        i = i + 1
    }
    var k: usize = 0
    while k < 6 {
        put(hist[k])
        k = k + 1
    }
    return 0
}
"""
T_PREFIX = """fn main() -> i32 {
    var a: [i32; @N@] = [0; @N@]
    var i: usize = 0
    while i < @N@ {
        a[i] = ((i * @A@) % 101) as i32 - 50
        i = i + 1
    }
    var p: i32 = 0
    var best: i32 = -1000000
    var worst: i32 = 1000000
    var j: usize = 0
    while j < @N@ {
        p = p + a[j]
        if p > best { best = p }
        if p < worst { worst = p }
        j = j + 1
    }
    put(p as i64)
    put(best as i64)
    put(worst as i64)
    return 0
}
"""
params = [
    ("sieve", T_SIEVE, [dict(N=n) for n in (50, 1000, 20000, 100000)]),
    ("collatz", T_COLLATZ, [dict(LO=1, HI=h) for h in (30, 300, 3000)] + [dict(LO=27, HI=28)]),
    ("gcd", T_GCD, [dict(N=n, A=a, B=b) for n, a, b in ((50, 6, 35), (500, 12, 77), (2000, 9, 1001))]),
    ("lcg", T_LCG, [dict(SEED=s, N=n) for s, n in ((1, 10), (12345, 1000), (987654321, 100000), (0, 5))]),
    ("sort", T_SORT, [dict(N=n, SEED=s) for n, s in ((5, 1), (32, 77), (100, 4242), (200, 9))]),
    ("matmul", T_MAT, [dict(A=a, B=b) for a, b in ((1, 1), (3, 5), (7, 2), (11, 13))]),
    ("bits", T_BITS, [dict(SEED=s, N=n) for s, n in ((3, 100), (99, 5000), (123456789, 20000))]),
    ("fib", T_FIB, [dict(A=a, B=b, N=n) for a, b, n in ((0, 1, 10), (0, 1, 90), (1, 3, 200), (5, 7, 1000))]),
    ("bsearch", T_BSEARCH, [dict(N=n, STEP=s) for n, s in ((10, 3), (100, 2), (1000, 7))]),
    ("lcs", T_LCS, [dict(N=n, SEED=s) for n, s in ((8, 1), (30, 5), (60, 99))]),
    ("branch", T_BRANCH, [dict(SEED=s, N=n) for s, n in ((1, 100), (7, 10000), (31337, 50000))]),
    ("prefix", T_PREFIX, [dict(N=n, A=a) for n, a in ((10, 3), (500, 17), (5000, 31))]),
]
for nm, t, ps in params:
    for i, p in enumerate(ps):
        emit("opt_%s_%d" % (nm, i), "run", "same", tpl(t, **p))

# ---------------------------------------------------------------- negative (rejected programs)
NEG = [
    ("syn_missing_brace", "fn main() -> i32 {\n    return 1\n"),
    ("syn_extra_brace", "fn main() -> i32 {\n    return 1\n}\n}\n"),
    ("syn_missing_operand", "fn main() -> i32 {\n    let x: i32 = 5 +\n    return x\n}\n"),
    ("syn_missing_paren", "fn main() -> i32 {\n    return (1 + 2\n}\n"),
    ("syn_let_no_name", "fn main() -> i32 {\n    let: i32 = 1\n    return 0\n}\n"),
    ("syn_fn_no_body", "fn main() -> i32\n"),
    ("syn_bad_token", "fn main() -> i32 {\n    return 1 @ 2\n}\n"),
    ("syn_if_no_block", "fn main() -> i32 {\n    if 1 < 2 return 1\n    return 0\n}\n"),
    ("syn_while_no_cond", "fn main() -> i32 {\n    while {}\n    return 0\n}\n"),
    ("syn_double_op", "fn main() -> i32 {\n    return 1 + *2\n}\n"),
    ("syn_unclosed_array", "fn main() -> i32 {\n    let a: [i32; 3] = [1, 2, 3\n    return 0\n}\n"),
    ("syn_call_unclosed", "fn f(a: i32) -> i32 { return a }\nfn main() -> i32 {\n    return f(1\n}\n"),
    ("syn_else_alone", "fn main() -> i32 {\n    else { return 1 }\n    return 0\n}\n"),
    ("syn_param_no_type", "fn f(a) -> i32 { return 1 }\nfn main() -> i32 {\n    return f(1)\n}\n"),
    ("syn_return_type_missing", "fn main() -> {\n    return 0\n}\n"),
    ("syn_empty_expr_stmt", "fn main() -> i32 {\n    let x: i32 =\n    return 0\n}\n"),
    ("syn_cast_no_type", "fn main() -> i32 {\n    return 1 as\n}\n"),
    ("syn_index_unclosed", "fn main() -> i32 {\n    var a: [i32; 2] = [1, 2]\n    return a[0\n}\n"),
    ("syn_stray_close_paren", "fn main() -> i32 {\n    return 1)\n}\n"),
    ("type_i32_bool", "fn main() -> i32 {\n    let x: i32 = true\n    return x\n}\n"),
    ("type_bool_int", "fn main() -> i32 {\n    let x: bool = 1\n    return 0\n}\n"),
    ("type_unknown_name", "fn main() -> i32 {\n    return zzz\n}\n"),
    ("type_unknown_fn", "fn main() -> i32 {\n    return nofn(1)\n}\n"),
    ("type_unknown_type", "fn main() -> i32 {\n    let x: Foo = 1\n    return 0\n}\n"),
    ("type_unknown_field", "struct S { a: i32 }\nfn main() -> i32 {\n    let s: S = S { a: 1 }\n    return s.b\n}\n"),
    ("type_missing_field", "struct S { a: i32, b: i32 }\nfn main() -> i32 {\n    let s: S = S { a: 1 }\n    return s.a\n}\n"),
    ("type_extra_field", "struct S { a: i32 }\nfn main() -> i32 {\n    let s: S = S { a: 1, b: 2 }\n    return s.a\n}\n"),
    ("type_arg_count_few", "fn f(a: i32, b: i32) -> i32 { return a + b }\nfn main() -> i32 {\n    return f(1)\n}\n"),
    ("type_arg_count_many", "fn f(a: i32) -> i32 { return a }\nfn main() -> i32 {\n    return f(1, 2)\n}\n"),
    ("type_arg_type", "fn f(a: i32) -> i32 { return a }\nfn main() -> i32 {\n    return f(true)\n}\n"),
    ("type_return_mismatch", "fn f() -> bool { return 1 }\nfn main() -> i32 {\n    return 0\n}\n"),
    ("type_return_missing_value", "fn f() -> i32 { return }\nfn main() -> i32 {\n    return f()\n}\n"),
    ("type_assign_immutable", "fn main() -> i32 {\n    let x: i32 = 1\n    x = 2\n    return x\n}\n"),
    ("type_mixed_width_add", "fn main() -> i32 {\n    let a: i32 = 1\n    let b: i64 = 2\n    return (a + b) as i32\n}\n"),
    ("type_mixed_sign_add", "fn main() -> i32 {\n    let a: i32 = 1\n    let b: u32 = 2\n    return (a + b) as i32\n}\n"),
    ("type_bool_arith", "fn main() -> i32 {\n    let a: bool = true\n    let b: bool = false\n    return (a + b) as i32\n}\n"),
    ("type_if_cond_int", "fn main() -> i32 {\n    if 1 {\n        return 1\n    }\n    return 0\n}\n"),
    ("type_while_cond_int", "fn main() -> i32 {\n    while 1 {\n        return 1\n    }\n    return 0\n}\n"),
    ("type_literal_u8_big", "fn main() -> i32 {\n    let y: u8 = 300\n    return y as i32\n}\n"),
    ("type_literal_u16_big", "fn main() -> i32 {\n    let y: u16 = 70000\n    return y as i32\n}\n"),
    ("type_literal_i8_very_neg", "fn main() -> i32 {\n    let y: i8 = 0 - 300\n    return y as i32\n}\n"),
    ("type_literal_cast_big", "fn main() -> i32 {\n    let y: u8 = 300 as u8\n    return y as i32\n}\n"),
    ("type_dup_fn", "fn f() -> i32 { return 1 }\nfn f() -> i32 { return 2 }\nfn main() -> i32 {\n    return f()\n}\n"),
    ("type_dup_struct", "struct S { a: i32 }\nstruct S { b: i32 }\nfn main() -> i32 {\n    return 0\n}\n"),
    ("type_dup_local", "fn main() -> i32 {\n    let a: i32 = 1\n    let a: i32 = 2\n    return a\n}\n"),
    ("type_dup_param", "fn f(a: i32, a: i32) -> i32 { return a }\nfn main() -> i32 {\n    return f(1, 2)\n}\n"),
    ("type_index_non_array", "fn main() -> i32 {\n    let a: i32 = 1\n    return a[0]\n}\n"),
    ("type_index_signed", "fn main() -> i32 {\n    var a: [i32; 2] = [1, 2]\n    let i: i32 = 0\n    return a[i]\n}\n"),
    ("type_array_len_mismatch", "fn main() -> i32 {\n    let a: [i32; 3] = [1, 2]\n    return 0\n}\n"),
    ("type_array_elem_mismatch", "fn main() -> i32 {\n    let a: [i32; 2] = [1, true]\n    return 0\n}\n"),
    ("type_field_on_int", "fn main() -> i32 {\n    let a: i32 = 1\n    return a.x\n}\n"),
    ("type_call_non_fn", "fn main() -> i32 {\n    let a: i32 = 1\n    return a(2)\n}\n"),
    ("type_deref_int", "fn main() -> i32 {\n    let a: i32 = 1\n    return *a\n}\n"),
    ("type_cast_struct_int", "struct S { a: i32 }\nfn main() -> i32 {\n    let s: S = S { a: 1 }\n    return s as i32\n}\n"),
    ("type_implicit_widen", "fn f(a: i64) -> i64 { return a }\nfn main() -> i32 {\n    let x: i32 = 1\n    return f(x) as i32\n}\n"),
    ("type_not_on_int", "fn main() -> i32 {\n    let a: i32 = 1\n    if !a { return 1 }\n    return 0\n}\n"),
    ("type_tilde_on_bool", "fn main() -> i32 {\n    let a: bool = true\n    let b: bool = ~a\n    return 0\n}\n"),
    ("type_compare_mixed", "fn main() -> i32 {\n    let a: i32 = 1\n    let b: i64 = 1\n    if a == b { return 1 }\n    return 0\n}\n"),
    ("type_main_wrong_ret", "fn main() -> bool {\n    return true\n}\n"),
    ("type_unknown_import", "import no.such.module\nfn main() -> i32 {\n    return 0\n}\n"),
    ("type_shift_bool", "fn main() -> i32 {\n    let a: bool = true\n    return (a << 1) as i32\n}\n"),
    ("type_assign_wrong_type", "fn main() -> i32 {\n    var a: i32 = 1\n    a = true\n    return a\n}\n"),
    ("type_struct_wrong_field_type", "struct S { a: i32 }\nfn main() -> i32 {\n    let s: S = S { a: true }\n    return s.a\n}\n"),
    ("type_break_outside_loop", "fn main() -> i32 {\n    break\n    return 0\n}\n"),
    ("type_unit_value", "fn f() {}\nfn main() -> i32 {\n    let x: i32 = f()\n    return x\n}\n"),
    ("type_ptr_to_int_assign", "fn main() -> i32 {\n    var a: [u8; 2] = [1, 2]\n    let p: u64 = &a[0]\n    return 0\n}\n"),
    ("type_closure_capture_big", "struct S { a: i64, b: i64 }\nfn main() -> i32 {\n    let s: S = S { a: 1, b: 2 }\n    let f: fn() -> i64 = gc fn() -> i64 { return s.a }\n    return 0\n}\n"),
    ("type_closure_capture_no_gc", "fn main() -> i32 {\n    let x: i64 = 4\n    let f: fn() -> i64 = fn() -> i64 { return x }\n    return 0\n}\n"),
    ("type_closure_gc_no_capture", "fn main() -> i32 {\n    let f: fn() -> i64 = gc fn() -> i64 { return 1 }\n    return 0\n}\n"),
    ("type_enum_ctor_1", "enum E { A, B(i64) }\nfn main() -> i32 {\n    let x: E = Nope::A\n    return 0\n}\n"),
    ("type_enum_ctor_2", "enum E { A, B(i64) }\nfn main() -> i32 {\n    let x: E = E::Z\n    return 0\n}\n"),
    ("type_enum_ctor_3", "enum E { A, B(i64) }\nfn main() -> i32 {\n    let x: E = E::B(1, 2)\n    return 0\n}\n"),
    ("type_enum_ctor_4", "enum E { A, B(i64) }\nfn main() -> i32 {\n    let x: E = E::B(true)\n    return 0\n}\n"),
    ("type_expr_ei1", "fn main() -> i32 {\n    let c: bool = true\n    let x: i64 = if c { 1 } else { true }\n    return 0\n}\n"),
    ("type_expr_ei2", "fn main() -> i32 {\n    let x: i64 = syscall()\n    return 0\n}\n"),
    ("type_expr_ei3", "struct P { a: i64 }\nfn main() -> i32 {\n    let p: P = P { a: 1 }\n    let x: i64 = syscall(60, p)\n    return 0\n}\n"),
    ("type_expr_ei4", "profile kernel\nfn main() -> i32 {\n    let x: i64 = syscall(60, 0)\n    return 0\n}\n"),
    ("type_expr_ei5", "fn main() -> i32 {\n    let a: bool = true\n    let x: u8 = a as% u8\n    return 0\n}\n"),
    ("type_expr_ei6", "fn main() -> i32 {\n    let a: [i64; 4] = [0; true]\n    return 0\n}\n"),
    ("type_expr_ei7", "fn main() -> i32 {\n    let a: [i64; 4] = [0; 0]\n    return 0\n}\n"),
    ("type_fnvalue_callee_arg_type", "fn inc(a: i64) -> i64 { return a + 1 }\nfn main() -> i32 {\n    var fs: [fn(i64) -> i64; 2] = [inc, inc]\n    let z: i64 = fs[0](true)\n    return 0\n}\n"),
    ("type_fnvalue_callee_count", "fn inc(a: i64) -> i64 { return a + 1 }\nfn main() -> i32 {\n    var fs: [fn(i64) -> i64; 2] = [inc, inc]\n    let z: i64 = fs[0](1, 2)\n    return 0\n}\n"),
    ("type_fnvalue_callee_not_fn", "fn main() -> i32 {\n    var fs: [i64; 2] = [1, 2]\n    let z: i64 = fs[0](1)\n    return 0\n}\n"),
    ("type_fnvalue_field_arg_count", "fn inc(a: i64) -> i64 { return a + 1 }\nstruct H { cb: fn(i64) -> i64 }\nfn main() -> i32 {\n    let h: H = H { cb: inc }\n    let q: i64 = h.cb()\n    let ff: fn(i64) -> i64 = inc\n    let z: i64 = (ff)(1, 2)\n    let y: i64 = (ff)(true)\n    return 0\n}\n"),
    ("type_fnvalue_field_arg_type", "fn inc(a: i64) -> i64 { return a + 1 }\nstruct H { cb: fn(i64) -> i64 }\nfn main() -> i32 {\n    let h: H = H { cb: inc }\n    let r: i64 = h.cb(true)\n    let q: i64 = h.cb()\n    return 0\n}\n"),
    ("type_iface_cast_1", "interface Name {\n    fn id(*self) -> i64\n}\nstruct Dot { x: i64 }\nimpl Name for Dot {\n    fn id(*self) -> i64 { return (*self).x }\n}\nfn main() -> i32 {\n    let n: i64 = 3\n    let d: dyn Name = n as dyn Name\n    return 0\n}\n"),
    ("type_iface_cast_2", "interface Name {\n    fn id(*self) -> i64\n}\nstruct Dot { x: i64 }\nimpl Name for Dot {\n    fn id(*self) -> i64 { return (*self).x }\n}\nfn main() -> i32 {\n    var n: i64 = 3\n    let d: dyn Name = (&n) as dyn Name\n    return 0\n}\n"),
    ("type_iface_cast_3", "interface Name {\n    fn id(*self) -> i64\n}\nstruct Dot { x: i64 }\nstruct Other { x: i64 }\nimpl Name for Dot {\n    fn id(*self) -> i64 { return (*self).x }\n}\nfn main() -> i32 {\n    var o: Other = Other { x: 3 }\n    let d: dyn Name = (&o) as dyn Name\n    return 0\n}\n"),
    ("type_kernel_asm1", "fn main() -> i32 {\n    asm(\"nop\", clobber(\"rbx\"))\n    return 0\n}\n"),
    ("type_kernel_asm2", "fn main() -> i32 {\n    asm(\"nop\", clobber(\"zzz\"))\n    return 0\n}\n"),
    ("type_kernel_asm3", "fn main() -> i32 {\n    let x: u64 = 1\n    asm(\"nop\", in(\"rbx\") x)\n    return 0\n}\n"),
    ("type_kernel_asm4", "fn main() -> i32 {\n    let x: u64 = 1\n    asm(\"nop\", in(\"rfoo\") x)\n    return 0\n}\n"),
    ("type_kernel_asm5", "fn main() -> i32 {\n    var v: u64 = 0\n    var w: u64 = 0\n    asm(\"nop\", out(\"rax\") &v, out(\"eax\") &w)\n    return 0\n}\n"),
    ("type_kernel_asm6", "struct P { a: u64, b: u64 }\nfn main() -> i32 {\n    let p: P = P { a: 1, b: 2 }\n    asm(\"nop\", in(\"rax\") p)\n    return 0\n}\n"),
    ("type_kernel_asm7", "fn main() -> i32 {\n    var v: u64 = 0\n    asm(\"nop\", out(\"rax\") v)\n    return 0\n}\n"),
    ("type_kernel_asm8", "fn main() -> i32 {\n    var v: u32 = 0\n    asm(\"nop\", out(\"rax\") &v)\n    return 0\n}\n"),
    ("type_kernel_mm1", "fn main() -> i32 {\n    let x: u32 = __mmio_read32()\n    return 0\n}\n"),
    ("type_kernel_mm2", "fn main() -> i32 {\n    __mmio_write8(1)\n    return 0\n}\n"),
    ("type_kernel_mm3", "fn main() -> i32 {\n    let a: u64 = 5\n    let x: u32 = __mmio_read32(a)\n    return 0\n}\n"),
    ("type_kernel_mm4", "fn main() -> i32 {\n    let a: u64 = 5\n    __mmio_write8(a, 1)\n    return 0\n}\n"),
    ("type_kernel_mm5", "fn main() -> i32 {\n    var p: u8 = 0\n    __mmio_write8(&p, true)\n    return 0\n}\n"),
    ("type_kernel_mm6", "fn main() -> i32 {\n    var p: u8 = 0\n    let q: u16 = 3\n    __mmio_write8(&p, q)\n    return 0\n}\n"),
    ("type_method_arg_5", "struct A { v: i32 }\nimpl A {\n    fn put(*self, x: i32, y: i32 = 1) -> i32 {\n        return (*self).v + x + y\n    }\n}\nfn main() -> i32 {\n    var a: A = A { v: 1 }\n    return a.put()\n}\n"),
    ("type_method_arg_6", "struct A { v: i32 }\nimpl A {\n    fn put(*self, x: i32, y: i32 = 1) -> i32 {\n        return (*self).v + x + y\n    }\n}\nfn main() -> i32 {\n    var a: A = A { v: 1 }\n    return a.put(1, 2, 3)\n}\n"),
    ("type_method_arg_7", "struct A { v: i32 }\nimpl A {\n    fn put(*self, x: i32, y: i32 = 1) -> i32 {\n        return (*self).v + x + y\n    }\n}\nfn main() -> i32 {\n    var a: A = A { v: 1 }\n    return a.put(true, 2)\n}\n"),
    ("type_struct_unknown_type", "fn main() -> i32 {\n    let x: i64 = Nope { a: 1 }.a\n    return 0\n}\n"),
    ("type_tuple_elem_type", "fn f() {}\nfn main() -> i32 {\n    let t: (i64, i64) = (1, f())\n    return 0\n}\n"),
    ("type_tuple_elem_void", "fn f() {}\nfn main() -> i32 {\n    let t = (1, f())\n    return 0\n}\n"),
    ('type_pat_bool_vs_int', 'fn main() -> i32 {\n    let x: i32 = 1\n    match x {\n        true => { }\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_pat_int_vs_bool', 'fn main() -> i32 {\n    let x: bool = true\n    match x {\n        1 => { }\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_pat_int_overflow', 'fn main() -> i32 {\n    let x: u8 = 1 as u8\n    match x {\n        300 => { }\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_pat_range_vs_bool', 'fn main() -> i32 {\n    let x: bool = true\n    match x {\n        1..5 => { }\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_pat_range_overflow', 'fn main() -> i32 {\n    let x: u8 = 1 as u8\n    match x {\n        1..300 => { }\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_pat_enum_unknown', 'enum E { A, B(i64), C }\nfn main() -> i32 {\n    let x: E = E::A\n    match x {\n        Nope::A => { }\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_pat_enum_wrong_type', 'enum E { A, B(i64), C }\nfn main() -> i32 {\n    let x: i32 = 1\n    match x {\n        E::A => { }\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_pat_enum_other_enum', 'enum E { A, B(i64), C }\nenum F { X, Y }\nfn main() -> i32 {\n    let x: E = E::A\n    match x {\n        F::X => { }\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_pat_enum_no_variant', 'enum E { A, B(i64), C }\nfn main() -> i32 {\n    let x: E = E::A\n    match x {\n        E::Zed => { }\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_pat_enum_sub_count', 'enum E { A, B(i64), C }\nfn main() -> i32 {\n    let x: E = E::A\n    match x {\n        E::B(a, b) => { }\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_match_on_struct', 'struct S { a: i32 }\nfn main() -> i32 {\n    let s: S = S { a: 1 }\n    match s {\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_match_on_ptr', 'fn main() -> i32 {\n    var v: i32 = 1\n    let p: *mut i32 = &v\n    match p {\n        _ => { }\n    }\n    return 0\n}\n'),
    ('type_match_unreachable', 'enum E { A, B(i64), C }\nfn main() -> i32 {\n    let x: E = E::A\n    match x {\n        _ => { }\n        E::A => { }\n    }\n    return 0\n}\n'),
    ('type_match_int_not_exhaustive', 'fn main() -> i32 {\n    let x: i32 = 1\n    match x {\n        1 => { }\n        2 => { }\n    }\n    return 0\n}\n'),
    ('type_match_bool_missing_true', 'fn main() -> i32 {\n    let x: bool = true\n    match x {\n        false => { }\n    }\n    return 0\n}\n'),
    ('type_match_bool_missing_false', 'fn main() -> i32 {\n    let x: bool = true\n    match x {\n        true => { }\n    }\n    return 0\n}\n'),
    ('type_match_enum_missing_one', 'enum E { A, B(i64), C }\nfn main() -> i32 {\n    let x: E = E::A\n    match x {\n        E::A => { }\n        E::B(v) => { }\n    }\n    return 0\n}\n'),
    ('type_match_enum_missing_two', 'enum E { A, B(i64), C }\nfn main() -> i32 {\n    let x: E = E::A\n    match x {\n        E::A => { }\n    }\n    return 0\n}\n'),
    ('type_err_try_not_union', 'fn f() -> i32 {\n    return 1\n}\nerror TE { Bad, Worse }\nerror TF { Other }\nfn g() -> TE!i32 {\n    let v: i32 = try f()\n    return v\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('type_err_try_in_plain_fn', 'error TE { Bad, Worse }\nerror TF { Other }\nfn f() -> TE!i32 {\n    return 1\n}\nfn g() -> i32 {\n    let v: i32 = try f()\n    return v\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('type_err_try_other_set', 'error TE { Bad, Worse }\nerror TF { Other }\nfn f() -> TE!i32 {\n    return 1\n}\nfn g() -> TF!i32 {\n    let v: i32 = try f()\n    return v\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('type_err_catch_not_union', 'error TE { Bad, Worse }\nerror TF { Other }\nfn main() -> i32 {\n    let v: i32 = 1 catch 2\n    return 0\n}\n'),
    ('type_err_catch_wrong_type', 'error TE { Bad, Worse }\nerror TF { Other }\nfn f() -> TE!i32 {\n    return 1\n}\nfn main() -> i32 {\n    let v: i32 = f() catch true\n    return 0\n}\n'),
    ('type_err_ctor_payload', 'error TE { Bad, Worse }\nerror TF { Other }\nfn f() -> TE!i32 {\n    return TE::Bad(1)\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('type_err_ctor_unknown_variant', 'error TE { Bad, Worse }\nerror TF { Other }\nfn f() -> TE!i32 {\n    return TE::Nope\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('type_err_coerce_other_set', 'error TE { Bad, Worse }\nerror TF { Other }\nfn f() -> TE!i32 {\n    return TF::Other\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('type_err_coerce_wrong_value', 'error TE { Bad, Worse }\nerror TF { Other }\nfn f() -> TE!i32 {\n    return true\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('type_stmt_let_void', 'fn v() {}\nfn main() -> i32 {\n    let x = v()\n    return 0\n}\n'),
    ('type_stmt_tuple_count', 'fn t() -> (i32, i32) {\n    return (1, 2)\n}\nfn main() -> i32 {\n    let (a, b, c) = t()\n    return 0\n}\n'),
    ('type_stmt_tuple_not_tuple', 'fn main() -> i32 {\n    let (a, b) = 5\n    return 0\n}\n'),
    ('type_stmt_ptr_add_assign_bool', 'fn main() -> i32 {\n    var v: i32 = 1\n    var p: *mut i32 = &v\n    p += true\n    return 0\n}\n'),
    ('type_stmt_assign_op_type', 'fn main() -> i32 {\n    var x: i32 = 1\n    x += 2.0\n    return 0\n}\n'),
    ('type_stmt_step_bool', 'fn main() -> i32 {\n    var b: bool = true\n    b++\n    return 0\n}\n'),
    ('type_stmt_step_down_f64', 'fn main() -> i32 {\n    var b: f64 = 1.0\n    b--\n    return 0\n}\n'),
    ('type_stmt_for_mixed', 'fn main() -> i32 {\n    for i in 0 as i32..5 as i64 {\n    }\n    return 0\n}\n'),
    ('type_stmt_return_value_in_void', 'fn f() {\n    return 1\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('type_stmt_discard_must_consume', '#[must_consume]\nfn f() -> i32 {\n    return 1\n}\nfn main() -> i32 {\n    f()\n    return 0\n}\n'),
    ('type_stmt_discard_must_consume_type', '#[must_consume]\nstruct R { a: i32 }\nfn f() -> R {\n    return R { a: 1 }\n}\nfn main() -> i32 {\n    f()\n    return 0\n}\n'),
    ('type_stmt_discard_error_union', 'error TE { Bad, Worse }\nerror TF { Other }\nfn f() -> TE!i32 {\n    return 1\n}\nfn main() -> i32 {\n    f()\n    return 0\n}\n'),
    ('type_text_wide_no_array', 'fn main() -> i32 {\n    let t = u"ab"\n    return 0\n}\n'),
    ('type_text_empty_array', 'fn main() -> i32 {\n    let t: [u8; 4] = ""\n    return 0\n}\n'),
    ('syn_method_param_list', 'struct S { a: i32 }\nimpl S {\n    fn m(self: *mut S, x: i32) -> i32 { return 1 }\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('type_const_01', 'const A: i32 = 1 / 0\nfn main() -> i32 { return A }\n'),
    ('type_const_02', 'const A: i32 = 1 << 200\nfn main() -> i32 { return A }\n'),
    ('type_const_03', 'const A: f64 = 1\nfn main() -> i32 { return 0 }\n'),
    ('type_const_04', 'const A: i32 = B + 1\nfn main() -> i32 { return A }\n'),
    ('type_const_05', 'const A: u8 = 1\nconst A: u8 = 2\nfn main() -> i32 { return 0 }\n'),
    ('type_const_06', 'const A: bool = 5\nfn main() -> i32 { return 0 }\n'),
    ('type_const_07', 'const A: str = 5 + 1\nfn main() -> i32 { return 0 }\n'),
    ('type_const_08', 'const A: *u8 = 0\nfn main() -> i32 { return 0 }\n'),
    ('type_const_09', 'const A: f32 = 1.0 % 2.0\nfn main() -> i32 { return 0 }\n'),
    ('type_const_10', 'const A: f64 = 1.5\nconst B: f32 = A\nfn main() -> i32 { return 0 }\n'),
    ('type_const_11', 'const A: i32 = 1\nconst B: f64 = A\nfn main() -> i32 { return 0 }\n'),
    ('type_const_12', 'const A: i32 = -(-4) +| 3\nfn main() -> i32 { return 0 }\n'),
    ('type_kernel_gc_01', 'profile kernel\ngc class C { x: i32 }\nfn main() -> i32 {\n    return 0\n}\n'),
    ('type_kernel_gc_02', 'gc class C { x: i32 }\nprofile kernel\nfn main() -> i32 {\n    return 0\n}\n'),
    ('type_static_01', 'static G: [u8; 3] = [1, 2]\nfn main() -> i32 { return 0 }\n'),
    ('type_static_02', 'static G: i32 = 4\nstatic G: i32 = 5\nfn main() -> i32 { return 0 }\n'),
    ('type_static_03', 'static G: i32 = true\nfn main() -> i32 { return 0 }\n'),
    ('type_static_04', 'static G: i32 = 1 / 0\nfn main() -> i32 { return 0 }\n'),
    ('type_static_05', 'static G: *u8 = 5 as *u8\nfn main() -> i32 { return 0 }\n'),
    ('type_static_06', 'struct P { x: i32, y: i32 }\nstatic G: P = P { x: 1 }\nfn main() -> i32 { return 0 }\n'),
    ('type_static_07', 'static G: [u8; 3] = 5\nfn main() -> i32 { return 0 }\n'),
    ('type_static_08', 'static G: Foo = 5\nfn main() -> i32 { return 0 }\n'),
    ('type_static_09', 'static G: i32 = 5\nconst G: i32 = 5\nfn main() -> i32 { return 0 }\n'),
    ('type_static_10', 'static G: [i32; 2] = [1, foo()]\nfn foo() -> i32 { return 1 }\nfn main() -> i32 { return 0 }\n'),
    ('type_unknown_01', 'struct S { a: Nope }\nfn main() -> i32 { return 0 }\n'),
    ('type_unknown_02', 'fn f(x: Nope) -> i32 { return 0 }\nfn main() -> i32 { return 0 }\n'),
    ('reg_err_01', 'fn f() -> Nope!i32 {\n    return 1\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_gc_01', 'gc class A extends B {\n    x: i32\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_gc_02', 'gc class A extends B {\n    x: i32\n}\ngc class B extends A {\n    y: i32\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_gc_03', 'struct P {\n    x: i32\n}\ngc class A {\n    p: P\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_gc_04', 'gc class A {\n    x: i32\n}\nfn f(a: Gc[Nope]) {}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_gc_05', 'gc class A {\n    x: i32\n}\ngc class B extends A {\n    x: i32\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_gc_06', 'gc class A {\n    x: i32\n}\nfn f(a: A) {}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_if_01', 'struct P {\n    x: i32\n}\nimpl Nope for P {\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_if_02', 'interface I {\n    fn m(*self) -> i32\n}\nstruct P {\n    x: i32\n}\nimpl I for P {\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_if_03', 'interface I {\n    fn m(*self) -> i32\n}\nimpl I for Q {\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_if_04', 'interface I {\n    fn m(*self, a: i32) -> i32\n}\nstruct P {\n    x: i32\n}\nimpl I for P {\n    fn m(*self, a: i64) -> i32 {\n        return 1\n    }\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_if_05', 'interface I {\n    fn m(*self) -> i32\n}\nstruct P {\n    x: i32\n}\nimpl I for P {\n    fn m(*self) -> i64 {\n        return 1\n    }\n}\nfn main() -> i32 {\n    return 0\n}\n'),
    ('reg_if_06', 'interface I {\n    fn m(*self) -> i32\n}\nstruct P {\n    x: i32\n}\nimpl I for P {\n    fn m(*self, a: i32) -> i32 {\n        return 1\n    }\n}\nfn main() -> i32 {\n    return 0\n}\n'),
]
for nm, src in NEG:
    emit("neg_" + nm, "neg", "", src, prelude=False)

# sanity programs that MUST be accepted by both (guard against both-reject hiding in neg)
emit("ok_struct_no_comma", "run", "same", "struct S { a: i32 b: i32 }\nfn main() -> i32 {\n    let s: S = S { a: 1, b: 2 }\n    put((s.a + s.b) as i64)\n    return 0\n}\n")
emit("ok_empty_main", "run", "same", "fn main() -> i32 {\n    return 0\n}\n", prelude=False)

os.makedirs(out_dir, exist_ok=True)
for f in os.listdir(out_dir):
    if f.endswith(".fi"):
        os.remove(os.path.join(out_dir, f))
for i, name in enumerate(sorted(files)):
    with open(os.path.join(out_dir, "%s.fi" % name), "w") as fh:
        fh.write(files[name])
print("wrote %d programs to %s" % (len(files), out_dir))
