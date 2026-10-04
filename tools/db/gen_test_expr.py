#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/gen_test_expr.py > tests/2162_db_expr.fi -- constant expressions with SQLite's answers.
import sqlite3, sys

EXPRS = [
    "1 + 2 * 3", "(1 + 2) * 3", "10 - 2 - 3", "7 / 2", "-7 / 2", "7 % 3", "-7 % 3", "7.5 / 2", "2 * 3.5", "1 / 0", "1 % 0",
    "9223372036854775807 + 1", "-9223372036854775807 - 2", "4611686018427387904 * 2", "9223372036854775807 * -1", "-9223372036854775808",
    "1e3", "0x1F + 1", ".5 + .5", "1 << 62", "1 << 64", "-1 >> 70", "5 & 3", "5 | 3", "~5", "- - 5",
    "'1' + '2'", "'3' * '4'", "'abc' + 1", "'1.5' + 1", "' 7 ' + 1", "'0x10' + 1", "'1e2' + 0",
    "'a' || 'b'", "1 || 2", "1.5 || 'x'", "NULL || 'a'", "x'4142' || 'c'",
    "1 = 1.0", "1 = '1'", "'1' = 1", "'abc' < 'abd'", "'a' < 'B'", "x'41' = 'A'", "1 < 'a'", "'a' < x'00'", "NULL = NULL", "NULL IS NULL",
    "1 IS 1", "NULL IS 1", "1 IS NOT NULL", "1 IS TRUE", "0 IS FALSE", "NULL IS TRUE", "2 IS NOT FALSE",
    "NULL AND 0", "NULL AND 1", "NULL OR 1", "NULL OR 0", "NOT NULL", "NOT 0", "1 AND 2", "0 OR 0",
    "'abc' COLLATE NOCASE = 'ABC'", "'a ' COLLATE RTRIM = 'a'", "'abc' = 'ABC'",
    "3 BETWEEN 1 AND 5", "3 NOT BETWEEN 1 AND 5", "NULL BETWEEN 1 AND 2", "'b' BETWEEN 'a' AND 'c'",
    "3 IN (1, 2, 3)", "3 IN (1, 2, NULL)", "3 NOT IN (1, 2)", "NULL IN (1)", "3 IN ()", "'1' IN (1)",
    "CASE WHEN 1 > 2 THEN 'a' WHEN 2 > 1 THEN 'b' ELSE 'c' END", "CASE 3 WHEN 1 THEN 'x' WHEN 3 THEN 'y' END", "CASE WHEN 0 THEN 1 END",
    "'abc' LIKE 'A%'", "'abc' GLOB 'A*'", "'abc' LIKE 'a_c'", "'abc' LIKE '%'", "'日本語' LIKE '日%'", "'a%' LIKE 'a\\%' ESCAPE '\\'", "'abd' GLOB 'a[bc]d'", "'abd' GLOB 'a[^b]d'",
    "abs(-3)", "abs(-3.5)", "abs('-2')", "abs(NULL)", "length('日本語')", "length(x'0102')", "length(12345)", "lower('ABCÉ')", "upper('abcé')",
    "substr('hello', 2, 3)", "substr('hello', -3)", "substr('hello', 0)", "substr('hello', 2, -1)", "substr('日本語', 2, 1)",
    "instr('hello world', 'o')", "instr('abc', 'x')", "replace('banana', 'an', 'AN')", "trim('  x  ')", "ltrim('xxabc', 'x')", "rtrim('abcxx', 'x')",
    "typeof(1)", "typeof(1.0)", "typeof('a')", "typeof(x'')", "typeof(NULL)", "typeof(1 + 1.0)", "typeof(1 / 2)",
    "hex('abc')", "hex(255)", "quote('it''s')", "quote(NULL)", "quote(1.5)", "quote(x'4142')",
    "coalesce(NULL, NULL, 3)", "ifnull(NULL, 'x')", "nullif(1, 1)", "nullif(1, 2)", "iif(1 > 2, 'a', 'b')",
    "min(3, 1, 2)", "max(3, 1, 2)", "min(1, NULL)", "round(2.5)", "round(-2.5)", "round(2.675, 2)", "round(1234.5678, 1)", "round(NULL)",
    "CAST('12abc' AS INTEGER)", "CAST(3.9 AS INTEGER)", "CAST(-3.9 AS INTEGER)", "CAST('1e2' AS INTEGER)", "CAST(7 AS TEXT)", "CAST(1.5 AS TEXT)",
    "CAST('abc' AS BLOB)", "CAST(x'4142' AS TEXT)", "CAST('1.5' AS NUMERIC)", "CAST('3' AS REAL)", "CAST(NULL AS INTEGER)", "CAST(1e30 AS INTEGER)",
    "0.1 + 0.2", "1.0e15", "1.0e16", "100.0", "1e-4", "1e-5", "1.7976931348623157e308", "123456789.123456789", "1/3.0", "-0.0",
    "char(72, 105)", "unicode('日')", "printf('%d-%s-%5.2f|%-4d|%04d|%x|%%', 42, 'x', 3.14159, 7, 42, 255)", "zeroblob(3)", "length(randomblob(8))",
]

def fmt(v):
    if v is None:
        return "NULL"
    if isinstance(v, bytes):
        return "x'" + v.hex() + "'"
    if isinstance(v, float):
        if v == 0.0:
            return "0.0"
        t = "%.15g" % v
        if "e" in t:
            m, e = t.split("e")
            if "." not in m:
                m += ".0"
            return m + "e" + e
        if "." not in t and "inf" not in t and "nan" not in t:
            t += ".0"
        return t
    return str(v)

def lit(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'

c = sqlite3.connect(":memory:")
lines = []
for i, e in enumerate(EXPRS):
    try:
        v = c.execute("SELECT " + e).fetchone()[0]
        want = fmt(v)
    except sqlite3.Error as ex:
        want = "E:" + str(ex)
    lines.append(f'    check(&t, {lit(e)}, q(d, {lit("SELECT " + e)}), {lit(want)})')

print('''// expect_exit: 0
// tests/2162_db_expr.fi -- constant expressions through lib/db, each against the answer SQLite gave
// (tools/db/gen_test_expr.py wrote this file; tools/db/check_expr.py compares 22,000 more, bit
// for bit): arithmetic and its overflow, text to number, comparison across types and affinities,
// three-valued logic, LIKE/GLOB, CASE, IN, BETWEEN, CAST, the scalar functions.
import std.rt
import std.io
import std.fs
import db.dberr
import db.dbcore as dc
import db.sqlite as sq

struct Tally {
    total: i64,
    fails: i64,
}

fn check(t: *mut Tally, what: str, got: str, want: str) {
    (*t).total = (*t).total + 1
    if !got.equal(want) {
        (*t).fails = (*t).fails + 1
        io.print("FAIL ")
        io.print(what)
        io.print(": got [")
        io.print(got)
        io.print("] want [")
        io.print(want)
        io.print_line("]")
    }
}

fn q(d: *mut dc.Db, sql: str) -> str {
    let s: str = sq.db_query_all(d, sql) catch | e | ("E:" + sq.last_error_message())
    return s
}

fn scratch_path() -> str {
    var b: rt.Buf = rt.buf_new()
    let head: str = "/tmp/firn_db_2162_"
    rt.buf_push_bytes(&b, head.p as u64, head.length())
    rt.buf_push_dec_i64(&b, syscall(39, 0, 0, 0, 0, 0, 0))
    let s: str = __str_copy(str { p: rt.buf_data(&b), n: rt.buf_len(&b) })
    rt.buf_free(&b)
    return s
}

fn main() -> i32 {
    var t: Tally = Tally { total: 0, fails: 0 }
    let path: str = scratch_path()
    let d: *mut dc.Db = sq.db_open(path, sq.OPEN_CREATE) catch | e | (0 as *mut dc.Db)
    if (d as u64) == 0 {
        io.print_line("FAIL cannot create the database")
        return 2
    }''')
print("\n".join(lines))
print('''    sq.db_close(d)
    fs.remove(path) catch false
    io.fmt_print_line(io.fmt_number(io.fmt_text(io.fmt_number(io.fmt_text(io.fmt_new(), "checks ".p as u64, 7), t.total), " failed ".p as u64, 8), t.fails))
    if t.fails != 0 {
        return 1
    }
    return 0
}''')
