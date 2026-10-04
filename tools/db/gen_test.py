#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/gen_test.py -- writes the body of tests/2161_db_sql.fi: every statement is run in
# SQLite and the answer (rows as "a|b;c|d", or "E:<message>") becomes the expectation in the
# Firn test. Run:  python3 tools/db/gen_test.py > /tmp/body.fi
import sqlite3, sys

STEPS = [
    "CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER NOT NULL DEFAULT 5, b TEXT UNIQUE, c REAL, d BLOB, e TEXT DEFAULT 'x')",
    "CREATE INDEX t_a ON t(a)",
    "CREATE UNIQUE INDEX t_ce ON t(c, e)",
    "INSERT INTO t(a, b, c) VALUES (1, 'one', 1.5), (2, 'two', 2.5), (3, 'three', 3.5)",
    "INSERT INTO t(b) VALUES ('five')",
    "INSERT INTO t(a, b) VALUES (NULL, 'bad')",
    "INSERT INTO t(a, b) VALUES (9, 'one')",
    "INSERT INTO t(id, b) VALUES (1, 'dupid')",
    "INSERT OR IGNORE INTO t(a, b) VALUES (9, 'one')",
    "INSERT OR REPLACE INTO t(a, b, c) VALUES (7, 'one', 7.5)",
    "INSERT INTO t(a, b, c) VALUES (4, 'four', 1.5)",
    "SELECT id, a, b, c, e FROM t ORDER BY id",
    "SELECT typeof(a), typeof(b), typeof(c), typeof(d) FROM t WHERE b = 'two'",
    "UPDATE t SET a = a * 10 WHERE a < 4",
    "UPDATE t SET b = 'two' WHERE id = 1",
    "SELECT id, a FROM t ORDER BY a DESC, id",
    "DELETE FROM t WHERE b LIKE 't%'",
    "SELECT count(*), sum(a), min(b), max(b), avg(a) FROM t",
    "SELECT id FROM t WHERE a BETWEEN 5 AND 8 ORDER BY id",
    "SELECT 1 + 1, 7 / 2, 7 % 3, -7 / 2, 2.5 * 2, 'a' || 'b' || 1, 5 > 3, NULL IS NULL, 1 = NULL",
    "SELECT abs(-3), round(2.675, 2), length('日本語'), upper('abc'), substr('hello', 2, 3), coalesce(NULL, 'z'), typeof(1.0), hex('A')",
    "SELECT CAST('12abc' AS INTEGER), CAST(3.9 AS INTEGER), CAST(7 AS TEXT), CAST('x' AS BLOB), typeof(CAST(1 AS REAL))",
    "SELECT CASE WHEN 1 > 2 THEN 'a' WHEN 2 > 1 THEN 'b' ELSE 'c' END, CASE 3 WHEN 1 THEN 'x' WHEN 3 THEN 'y' END",
    "SELECT 9223372036854775807 + 1, 4611686018427387904 * 2, 1 / 0, 5 % 0",
    "SELECT 'abc' LIKE 'A%', 'abc' GLOB 'A*', 'a_c' LIKE 'a\\_c' ESCAPE '\\', 3 IN (1, 2, 3), 3 NOT IN (1, 2), 'b' BETWEEN 'a' AND 'c'",
    "CREATE TABLE p(id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, boss INTEGER REFERENCES p(id))",
    "INSERT INTO p(name, boss) VALUES ('ann', NULL), ('bob', 1), ('cy', 1), ('di', 2)",
    "DELETE FROM p WHERE id = 4",
    "INSERT INTO p(name, boss) VALUES ('ed', 3)",
    "SELECT * FROM p ORDER BY id",
    "SELECT name, seq FROM sqlite_sequence",
    "SELECT e.name, b.name FROM p e LEFT JOIN p b ON b.id = e.boss ORDER BY e.id",
    "SELECT b.name, count(*) FROM p e JOIN p b ON b.id = e.boss GROUP BY b.id ORDER BY b.name",
    "SELECT name FROM p WHERE boss IN (SELECT id FROM p WHERE boss IS NULL) ORDER BY name",
    "SELECT name, (SELECT count(*) FROM p x WHERE x.boss = p.id) FROM p ORDER BY id",
    "SELECT name FROM p WHERE EXISTS (SELECT 1 FROM p x WHERE x.boss = p.id) ORDER BY name",
    "SELECT name FROM p WHERE boss = 1 UNION SELECT name FROM p WHERE id = 1 ORDER BY 1",
    "SELECT boss FROM p WHERE boss IS NOT NULL EXCEPT SELECT id FROM p WHERE id = 1",
    "SELECT name FROM (SELECT name, id FROM p WHERE id > 1) WHERE id < 5 ORDER BY id",
    "SELECT group_concat(name, '+') FROM (SELECT name FROM p ORDER BY id)",
    "BEGIN",
    "INSERT INTO p(name) VALUES ('tx1')",
    "SELECT count(*) FROM p",
    "ROLLBACK",
    "SELECT count(*) FROM p",
    "BEGIN",
    "INSERT INTO p(name) VALUES ('tx2')",
    "INSERT INTO p(name) VALUES (NULL)",
    "COMMIT",
    "SELECT count(*) FROM p",
    "COMMIT",
    "ALTER TABLE p ADD COLUMN age INTEGER DEFAULT 18",
    "SELECT name, age FROM p ORDER BY id LIMIT 2",
    "UPDATE p SET age = 30 WHERE id = 2",
    "SELECT name, age FROM p ORDER BY id LIMIT 3",
    "CREATE TABLE u(k TEXT PRIMARY KEY, n INTEGER)",
    "INSERT INTO u VALUES ('a', 1)",
    "INSERT INTO u VALUES ('a', 5) ON CONFLICT(k) DO UPDATE SET n = n + excluded.n",
    "INSERT INTO u VALUES ('a', 5) ON CONFLICT(k) DO NOTHING",
    "SELECT * FROM u",
    "SELECT * FROM nosuch",
    "SELECT nosuch FROM t",
    "SELCT 1",
    "INSERT INTO t(nosuch) VALUES (1)",
    "INSERT INTO t VALUES (1, 2)",
    "CREATE TABLE t(x)",
    "DROP TABLE nosuch",
    "DROP TABLE IF EXISTS nosuch",
    "SELECT * FROM t WHERE",
    "SELECT a, b FROM t GROUP BY a HAVING count(*) > 5",
    "SELECT sqlite_version() > '3'",
    "DROP INDEX t_a",
    "DROP TABLE u",
    "SELECT type, name FROM sqlite_master ORDER BY name",
    "PRAGMA table_info(p)",
    "PRAGMA index_list(t)",
    "PRAGMA integrity_check",
]

def fmt(v):
    if v is None:
        return "NULL"
    if isinstance(v, bytes):
        return "x'" + v.hex() + "'"
    if isinstance(v, float):
        # SQLite's text form of a REAL: "%!.15g"
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

c = sqlite3.connect(":memory:", isolation_level=None)
def lit(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'
for i, sql in enumerate(STEPS):
    try:
        cur = c.execute(sql)
        rows = cur.fetchall() if cur.description else []
        want = ";".join("|".join(fmt(v) for v in r) for r in rows)
    except sqlite3.Error as e:
        want = "E:" + str(e)
    print(f'    check(&t, "step {i}", q(d, {lit(sql)}), {lit(want)})')
