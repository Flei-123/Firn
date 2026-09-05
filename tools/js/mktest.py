#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/js/mktest.py -- turn a JavaScript program into a Firn test.

The tests of the JavaScript path all have the same shape (round 63, round
66): ONE program with numbered assertions, the number of the FIRST failing
one lands in `globalThis.fail` and comes back as the exit code, so the code
names the check that broke.

Writing that by hand means escaping the program into a Firn text literal
and counting its octets -- twice, because the array length and the length
that is passed on are two independent numbers (docs/ROUND66.md, gap 10).
This produces it instead.

Usage:  python3 tools/js/mktest.py <out.fi> <title> <program.js>

The programs live in tools/js/progs/, NOT in tools/js/cases/: the latter is
what tools/js/compare_node.sh holds against node, and a program that uses
an ES2024 built in does not run on every node.
"""
import sys

HEAD = '''// expect_exit: 0
// %(path)s -- ROUND 74: %(title)s
//
// The engine runs ONE JavaScript program with numbered assertions; the
// number of the FIRST failing one lands in the global variable `fail` and
// comes back as the exit code, so the code names the check that broke.
// Produced by tools/js/mktest.py out of tools/js/progs/%(case)s.
import html.mem
import js.lex
import js.ast
import js.parse
import js.val
import js.interp
import js.builtin
import js.gen

const SRC_LEN: usize = %(len)d

fn __gc_finalize(kind: u64, p: *mut u8) {
}

fn run(src: *mut u8, len: usize) -> AllocError!u64 {
    var nm: lex.Names = lex.Names { pool: mem.cp_new(), off: mem.cp_new(),
        len: mem.cp_new(), hash: mem.cp_new(), buckets: mem.cp_new(),
        mask: 0, n: 0 }
    lex.names_init(&nm)
    var l: lex.Lexer = lex.lex_new()
    lex.lex_init(&l, &nm)
    var t: ast.Ast = ast.ast_new()
    ast.ast_init(&t)
    var p: parse.Parser = parse.parser_new()
    parse.parser_init(&p, &l, &t, &nm)
    lex.lex_set_ascii(&l, src, len)
    let prog: u32 = parse.parse_program(&p, false)
    if parse.parse_error(&p) != parse.ERR_NONE {
        return 900 + parse.parse_error_line(&p) as u64
    }
    var ctx: interp.Ctx = interp.ctx_new()
    ctx.native = builtin.native_dispatch
    ctx.arity = builtin.native_arity_of
    ctx.gen_start = gen.gen_start
    ctx.gen_native = gen.gen_native
    var sp: gen.Susp = gen.susp_new()
    gen.susp_init(&sp)
    ctx.gen_data = (&sp) as *mut u8
    let r: Gc[Realm] = try val.realm_new()
    val.realm_set_ctx(r, (&ctx) as *mut u8)
    val.realm_set_ast(r, &t, &nm)
    let g: Gc[JsObj] = try builtin.install(r)
    let okg: bool = try gen.install_gen(r)
    let v: Gc[JsVal] = try interp.run_program(r, prog, r.global_env)
    if ctx.comp == interp.C_THROW {
        return 800
    }
    let okj: bool = try gen.run_jobs(r)
    if ctx.comp == interp.C_THROW {
        return 801
    }
    var name: [u8; 4] = "fail"
    let s: Gc[JsVal] = try val.str_from_ascii(r, &name[0], 4)
    let k: u64 = try val.key_for_str(r, s)
    let gv: Gc[JsVal] = g
    let f: Gc[JsVal] = try interp.get_prop(r, gv, k)
    let n: u64 = try interp.to_number(r, f)
    let x: f64 = math_of(n)
    gen.susp_free(&sp)
    return x as u64
}

fn math_of(bits: u64) -> f64 {
    var t: u64 = bits
    return *(((&t) as usize) as *mut f64)
}

fn main() -> i32 {
    if !gc_init() {
        return 90
    }
    gc_set_max_bytes(1 << 30)
    var s: [u8; %(len)d] = "%(lit)s"
    let bad: u64 = run(&s[0], SRC_LEN) catch 999
    return bad as i32
}
'''


def escape(src):
    """The Firn text literal -- and the octet count it stands for."""
    out = []
    n = 0
    for ch in src:
        o = ord(ch)
        if o > 127:
            raise SystemExit("the program has to be ASCII: %r" % ch)
        if ch == '"':
            out.append('\\"')
        elif ch == '\\':
            out.append('\\\\')
        elif ch == '\n':
            out.append('\\n')
        elif ch == '\t':
            out.append('\\t')
        elif ch == '\r':
            out.append('\\r')
        elif o < 32:
            raise SystemExit("control character %d in the program" % o)
        else:
            out.append(ch)
        n += 1
    return "".join(out), n


def main():
    out, title, case = sys.argv[1], sys.argv[2], sys.argv[3]
    src = open(case, encoding="utf-8").read()
    lit, n = escape(src)
    text = HEAD % {"path": out, "title": title, "len": n, "lit": lit,
                   "case": case.rsplit("/", 1)[-1]}
    open(out, "w", encoding="utf-8").write(text)
    print("%s: %d octets" % (out, n))


main()
