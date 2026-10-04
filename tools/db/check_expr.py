#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/check_expr.py <expr_probe> -- constant expressions of lib/db against SQLite:
# literals, arithmetic with integer overflow, comparisons across types and affinities,
# casts, LIKE/GLOB, CASE, IN, BETWEEN, string and number functions. Thousands of
# expressions, generated; the result type AND the exact value (REAL by its bits).
import itertools, os, sqlite3, struct, subprocess, sys, tempfile

probe = sys.argv[1]
conn = sqlite3.connect(":memory:")

operands = ["0", "1", "-1", "2", "7", "-7", "255", "9223372036854775807", "-9223372036854775807", "4611686018427387904",
            "1.5", "-2.5", "0.1", "1e20", "3.0", "-0.0", "'abc'", "'ABC'", "''", "'12'", "'1.5e1'", "' 7 '", "'7x'", "'x7'",
            "'0x10'", "NULL", "x'4142'", "x''", "'a%'", "'é'", "TRUE", "FALSE"]
binops = ["+", "-", "*", "/", "%", "||", "=", "<>", "<", "<=", ">", ">=", "AND", "OR", "&", "|", "<<", ">>", "IS", "IS NOT"]
exprs = []
for a, b in itertools.product(operands, operands):
    for op in binops:
        exprs.append(f"({a}) {op} ({b})")
for a in operands:
    for f in ["-", "+", "NOT ", "~"]:
        exprs.append(f"{f}({a})")
    for f in ["abs", "length", "lower", "upper", "typeof", "hex", "quote", "round", "ifnull(NULL,", "trim", "ltrim", "rtrim", "unicode"]:
        exprs.append(f"{f}({a})" if not f.endswith(",") else f"{f}{a})")
    exprs.append(f"({a}) IS NULL")
    exprs.append(f"({a}) IS NOT NULL")
    exprs.append(f"({a}) ISNULL")
    exprs.append(f"({a}) NOTNULL")
    for t in ["INTEGER", "REAL", "TEXT", "BLOB", "NUMERIC"]:
        exprs.append(f"CAST(({a}) AS {t})")
    exprs.append(f"({a}) BETWEEN 1 AND 5")
    exprs.append(f"({a}) NOT BETWEEN 'a' AND 'b'")
    exprs.append(f"({a}) IN (1, 2, 'abc', NULL)")
    exprs.append(f"({a}) NOT IN (1, 2, 'abc')")
    exprs.append(f"({a}) IN ()")
    exprs.append(f"CASE WHEN ({a}) THEN 'y' ELSE 'n' END")
    exprs.append(f"CASE ({a}) WHEN 1 THEN 'one' WHEN 'abc' THEN 'abc' END")
    exprs.append(f"coalesce({a}, 5)")
    exprs.append(f"nullif({a}, 1)")
    exprs.append(f"iif({a}, 1, 2)")
    exprs.append(f"min({a}, 5, 1)")
    exprs.append(f"max({a}, 'a')")
for a in ["'abc'", "'ABC'", "'a%c'", "'abcdef'", "''", "NULL", "'日本語'", "'hello world'", "123", "'a_c'"]:
    for p in ["'a%'", "'%c'", "'a_c'", "'%'", "'_'", "'ABC'", "'%b%'", "'a\\%%'", "'日%'", "'___'", "''", "NULL"]:
        exprs.append(f"({a}) LIKE ({p})")
        exprs.append(f"({a}) NOT LIKE ({p})")
        exprs.append(f"({a}) GLOB ({p})")
        exprs.append(f"({a}) LIKE ({p}) ESCAPE '\\'")
for s in ["'hello'", "'日本語テキスト'", "'abc'", "''", "NULL", "12345", "x'0102030405'"]:
    for st, ln in [(1, 1), (2, 3), (0, 2), (-2, 5), (-3, 2), (3, -2), (10, 2), (1, 0), (-10, 4), (2, 100), (0, 0), (-1, -1)]:
        exprs.append(f"substr({s}, {st}, {ln})")
    for st in [1, 2, 0, -1, -3, 9]:
        exprs.append(f"substr({s}, {st})")
for a, b in [("'hello world'", "'o'"), ("'hello'", "'xyz'"), ("'abc'", "''"), ("'日本語'", "'語'"), ("x'010203'", "x'02'"), ("NULL", "'a'")]:
    exprs.append(f"instr({a}, {b})")
for a, b, c in [("'hello'", "'l'", "'L'"), ("'aaa'", "'a'", "'bb'"), ("'x'", "''", "'y'"), ("NULL", "'a'", "'b'"), ("'abc'", "'abc'", "''")]:
    exprs.append(f"replace({a}, {b}, {c})")
for a, b in [("'xxabcxx'", "'x'"), ("'  abc  '", "' '"), ("'abc'", "'abc'"), ("'abcba'", "'ab'"), ("'abc'", "''")]:
    exprs.append(f"trim({a}, {b})")
    exprs.append(f"ltrim({a}, {b})")
    exprs.append(f"rtrim({a}, {b})")
for x in ["2.5", "3.5", "-2.5", "1234.5678", "0.5", "1.005", "2.675", "NULL", "'3.7'", "1e300", "0"]:
    exprs.append(f"round({x})")
    for d in [0, 1, 2, 3, 10]:
        exprs.append(f"round({x}, {d})")
exprs += ["1 + 2 * 3", "(1 + 2) * 3", "10 - 2 - 3", "2 * 3 % 4", "- 2 * 3", "1 || 2 || 3", "-1 || 'a'", "1 + 2 || 3", "1 = 1 = 1",
          "NOT 1 = 2", "1 < 2 AND 2 < 3 OR 0", "1 OR 0 AND 0", "5 & 3 | 8", "1 << 2 + 1", "~5", "~~5", "- - 5",
          "9223372036854775807 + 1", "-9223372036854775807 - 2", "9223372036854775807 * 2", "4611686018427387904 * 2",
          "-9223372036854775808", "- 9223372036854775808", "9223372036854775808", "-9223372036854775809",
          "1 / 0", "1 % 0", "1.0 / 0", "0.0 / 0", "5 / 2", "-5 / 2", "5 % -3", "-5 % 3", "5.5 % 2", "7 / 2.0",
          "1 << 63", "1 << 64", "1 << -1", "-1 >> 70", "256 >> 4", "-256 >> 4",
          "0x10 + 1", "0xFF", "1e3", "1.5e-3", ".5 + .5", "5. + 1", "'1' + '2'", "'1' || '2'", "'3' * '4'", "'a' + 1",
          "1 = 1.0", "1 = '1'", "'1' = 1", "'abc' < 'abd'", "'a' < 'B'", "x'41' = 'A'", "x'41' < 'A'", "1 < 'a'", "'a' < x'00'",
          "NULL = NULL", "NULL IS NULL", "NULL IS NOT NULL", "1 IS 1", "1 IS NULL", "NULL IS 1",
          "1 IS NOT DISTINCT FROM 1", "NULL IS NOT DISTINCT FROM NULL", "1 IS DISTINCT FROM NULL",
          "abs(-5)", "abs(-5.5)", "abs('-3')", "abs(-9223372036854775807)", "abs(-9223372036854775808)",
          "typeof(1+1)", "typeof(1+1.0)", "typeof(1/2)", "typeof(1.0/2)", "typeof('a'||1)", "typeof(NULL)", "typeof(x'')",
          "length('日本語')", "length(x'0102')", "length(12345)", "length(1.5)", "length(NULL)",
          "hex('abc')", "hex(255)", "hex(x'ff00')", "hex(NULL)",
          "quote('it''s')", "quote(1.5)", "quote(NULL)", "quote(x'4142')", "quote(1)",
          "upper('abcé')", "lower('ABCÉ')", "char(65, 66, 0x65E5)", "unicode('日')", "unicode('')",
          "printf('%d-%s-%5.2f|%-4d|%04d|%x|%%', 42, 'x', 3.14159, 7, 42, 255)",
          "printf('%s', NULL)", "printf('%d', '12abc')", "printf('%.3f', 2.0005)", "printf('%5s|%-5s|', 'ab', 'cd')",
          "zeroblob(3)", "typeof(randomblob(4))", "length(randomblob(8))", "typeof(random())",
          "CAST('12abc' AS INTEGER)", "CAST('1e2' AS INTEGER)", "CAST(1.9 AS INTEGER)", "CAST(-1.9 AS INTEGER)", "CAST('  5  ' AS REAL)",
          "CAST(1e30 AS INTEGER)", "CAST(-1e30 AS INTEGER)", "CAST('9223372036854775808' AS INTEGER)", "CAST(x'4142' AS TEXT)",
          "CAST(123 AS TEXT)", "CAST(1.5 AS TEXT)", "CAST('abc' AS BLOB)", "CAST('1.0' AS NUMERIC)", "CAST('1.5' AS NUMERIC)",
          "CAST(5 AS NUMERIC)", "CAST('abc' AS NUMERIC)", "CAST(NULL AS TEXT)",
          "1 BETWEEN 0 AND 2", "1 BETWEEN 2 AND 0", "'b' BETWEEN 'a' AND 'c'", "NULL BETWEEN 1 AND 2", "1 BETWEEN NULL AND 2",
          "1 IN (1)", "1 IN (2, 3)", "1 IN (2, NULL)", "NULL IN (1)", "1 NOT IN (2, NULL)", "'1' IN (1)", "1 IN ('1')",
          "CASE WHEN 0 THEN 1 END", "CASE 2 WHEN 1 THEN 'a' WHEN 2 THEN 'b' ELSE 'c' END", "CASE NULL WHEN NULL THEN 1 ELSE 2 END",
          "'abc' COLLATE NOCASE = 'ABC'", "'abc' = 'ABC' COLLATE NOCASE", "'a ' COLLATE RTRIM = 'a'", "'abc' = 'ABC'",
          "'abc' LIKE 'ABC'", "'abc' GLOB 'ABC'", "'a[bc]d' GLOB 'a[bc]d'", "'abd' GLOB 'a[bc]d'", "'abd' GLOB 'a[^c]d'", "'a-d' GLOB 'a[a-z]d'",
          "'abc' LIKE 'a_c'", "'abc' LIKE '_'", "'' LIKE ''", "'%' LIKE '\\%' ESCAPE '\\'", "'a' LIKE 'a' || '%'",
          "like('a%', 'abc')", "glob('a*', 'abc')", "like('A%', 'abc', '')",
          "ifnull(NULL, NULL)", "coalesce(NULL, NULL, 3)", "nullif(1, 1)", "nullif(1, 2)", "iif(1 > 2, 'a', 'b')",
          "min(3, 1, 2)", "max(3, 1, 2)", "min(1, NULL)", "max('a', 'b')", "min(1, 1.0)", "max(1, 1.0)",
          "1 + NULL", "NULL || 'a'", "NOT NULL", "NULL AND 0", "NULL AND 1", "NULL OR 1", "NULL OR 0", "0 AND NULL", "1 OR NULL",
          "'' + 1", "' ' + 1", "'.5' + 1", "'5.' + 1", "'1e' + 1", "'-' + 1", "'+5' + 1", "'--5' + 1",
          "123456789012345678", "1234567890123456789012", "0.1 + 0.2", "1.0e15", "1.0e16", "123456789.123456789", "1/3.0", "2/3.0",
          "100.0", "1e-4", "1e-5", "12345678901234567890.0", "5e-324", "1.7976931348623157e308",
          "'abc' || 1.5", "1.0 || ''", "3.0 || 'x'", "1e100 || ''", "-0.0 || ''",
          "substr('abc', 2)", "substr('abc', 0)", "substr('abc', -1)", "substr('abc', 2, -1)", "substr('abc', 1, 1.9)",
          ]
# unique, in order
seen = set()
uniq = []
for e in exprs:
    if e not in seen:
        seen.add(e)
        uniq.append(e)
exprs = uniq

def enc(v):
    if v is None:
        return "N"
    if isinstance(v, bool):
        return f"I:{int(v)}"
    if isinstance(v, int):
        return f"I:{v}"
    if isinstance(v, float):
        return "R:" + struct.pack(">d", v).hex()
    if isinstance(v, str):
        return "T:" + v.encode("utf-8", "surrogatepass").hex()
    if isinstance(v, bytes):
        return "B:" + v.hex()
    return "?"

def sqlite_result(e):
    try:
        conn.text_factory = bytes
        cur = conn.execute("SELECT " + e)
        v = cur.fetchone()[0]
        if isinstance(v, bytes):
            # text came back as bytes: need the type, ask typeof
            t = conn.execute("SELECT typeof(" + e + ")").fetchone()[0]
            t = t.decode() if isinstance(t, bytes) else t
            if t == "text":
                return "T:" + v.hex()
            return "B:" + v.hex()
        return enc(v)
    except sqlite3.Error as ex:
        return "E"

skip_prefix = ("random()",)
with tempfile.TemporaryDirectory() as d:
    path = os.path.join(d, "exprs.txt")
    with open(path, "w", encoding="utf-8") as f:
        for e in exprs:
            f.write(e.replace("\n", " ") + "\n")
    r = subprocess.run([probe, path], capture_output=True)
    mine = r.stdout.decode("utf-8", "replace").split("\n")
fails = 0
for i, e in enumerate(exprs):
    want = sqlite_result(e)
    got = mine[i] if i < len(mine) else "?"
    if got.startswith("E:"):
        got = "E"
    if want != got:
        fails += 1
        if fails <= 60:
            print(f"DIFF {e!r}: sqlite={want} mine={got}")
print(f"{len(exprs)} expressions, {fails} differences")
sys.exit(1 if fails else 0)
