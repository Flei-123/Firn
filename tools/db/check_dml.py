#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/check_dml.py <sql_probe> -- INSERT / UPDATE / DELETE / DDL / transactions of lib/db
# against SQLite. The same statements run in both; every status and every result row is
# compared; then SQLite opens the file lib/db wrote (integrity_check, every table read back,
# the schema), and lib/db opens the file SQLite wrote.
import os, random, shutil, sqlite3, struct, subprocess, sys, tempfile

probe = sys.argv[1]
SEED = int(sys.argv[2]) if len(sys.argv) > 2 else 20261004
random.seed(SEED)

def enc(v):
    if v is None:
        return "N"
    if isinstance(v, int):
        return f"I:{v}"
    if isinstance(v, float):
        return "R:" + struct.pack(">d", v).hex()
    if isinstance(v, str):
        return "T:" + v.encode("utf-8").hex()
    return "B:" + bytes(v).hex()

def run_mine(db, stmts, crash=None):
    d = tempfile.mkdtemp()
    script = os.path.join(d, "s.sql")
    with open(script, "w", encoding="utf-8") as f:
        for s in stmts:
            f.write(s + "\n")
    args = [probe, db, script]
    r = subprocess.run(args, capture_output=True)
    shutil.rmtree(d)
    lines = r.stdout.decode("utf-8", "replace").split("\n")
    out = []
    pos = 0
    for s in stmts:
        rows = []
        while pos < len(lines) and lines[pos].startswith("R "):
            rows.append(lines[pos][2:])
            pos += 1
        status = lines[pos] if pos < len(lines) else "?"
        pos += 1
        out.append((rows, status))
    return out, r

def run_ref(conn, stmts):
    out = []
    for s in stmts:
        try:
            cur = conn.execute(s)
            rows = ["|".join(enc(v) for v in row) for row in cur.fetchall()] if cur.description else []
            out.append((rows, "OK"))
        except sqlite3.Error as e:
            out.append(([], "E " + str(e)))
    return out

fails = 0
def compare(label, stmts, mine, ref, ordered=False):
    global fails
    bad = 0
    for s, (mr, ms), (rr, rs) in zip(stmts, mine, ref):
        if (ms.startswith("E")) != (rs.startswith("E")):
            bad += 1
            print(f"  [{label}] STATUS {s!r}: sqlite={rs[:80]} mine={ms[:80]}")
            continue
        if ms.startswith("E"):
            continue
        a = mr if ordered else sorted(mr)
        b = rr if ordered else sorted(rr)
        if a != b:
            bad += 1
            print(f"  [{label}] ROWS {s!r}: mine {len(a)} sqlite {len(b)}")
            for x, y in zip(a, b):
                if x != y:
                    print("      first difference: mine", x[:160], " sqlite", y[:160])
                    break
    if bad:
        fails += bad
    return bad

def tables_of(conn):
    return [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")] + \
           [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE name = 'sqlite_sequence'")]

def verify_files(label, mine_path, ref_conn):
    """SQLite reads the file lib/db wrote and compares with its own result."""
    global fails
    c = sqlite3.connect(mine_path)
    ic = c.execute("PRAGMA integrity_check").fetchall()
    if ic != [("ok",)]:
        fails += 1
        print(f"  [{label}] integrity_check by SQLite: {ic[:3]}")
    names_mine = [(r[0], r[1], r[2]) for r in c.execute("SELECT type, name, tbl_name FROM sqlite_master ORDER BY name")]
    names_ref = [(r[0], r[1], r[2]) for r in ref_conn.execute("SELECT type, name, tbl_name FROM sqlite_master ORDER BY name")]
    if names_mine != names_ref:
        fails += 1
        print(f"  [{label}] schema differs: mine {names_mine} sqlite {names_ref}")
    for t in tables_of(ref_conn):
        try:
            a = sorted(map(repr, c.execute(f'SELECT rowid, * FROM "{t}"').fetchall()))
        except sqlite3.Error as e:
            fails += 1
            print(f"  [{label}] table {t} unreadable by SQLite: {e}")
            continue
        b = sorted(map(repr, ref_conn.execute(f'SELECT rowid, * FROM "{t}"').fetchall()))
        if a != b:
            fails += 1
            print(f"  [{label}] table {t}: mine {len(a)} rows, sqlite {len(b)} rows")
            for x, y in zip(a, b):
                if x != y:
                    print("      first difference: mine", x[:160], " sqlite", y[:160])
                    break
    c.close()

def scenario(label, stmts, ordered=False):
    with tempfile.TemporaryDirectory() as d:
        mine_db = os.path.join(d, "mine.db")
        ref_db = os.path.join(d, "ref.db")
        mine, r = run_mine(mine_db, stmts)
        ref_conn = sqlite3.connect(ref_db, isolation_level=None)
        ref = run_ref(ref_conn, stmts)
        n_bad = compare(label, stmts, mine, ref, ordered)
        if r.returncode != 0:
            print(f"  [{label}] probe exit {r.returncode}: {r.stderr.decode()[:200]}")
        verify_files(label, mine_db, ref_conn)
        # and the other way: lib/db reads (and appends to) the file SQLite wrote
        copy = os.path.join(d, "copy.db")
        ref_conn.close()
        shutil.copy(ref_db, copy)
        tail = ["SELECT count(*) FROM sqlite_master", "PRAGMA integrity_check"]
        out, r2 = run_mine(copy, tail)
        if r2.returncode != 0 or not out[1][0] == ["T:" + b"ok".hex()]:
            global fails
            fails += 1
            print(f"  [{label}] lib/db could not read the file SQLite wrote: {out[1]} {r2.stderr.decode()[:200]}")
        print(f"{'ok  ' if n_bad == 0 else 'FAIL'} {label}: {len(stmts)} statements")

S = []
S.append("CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER NOT NULL DEFAULT 5, b TEXT UNIQUE, c REAL, d BLOB, e TEXT DEFAULT 'x')")
S.append("CREATE INDEX t_a ON t(a)")
S.append("CREATE UNIQUE INDEX t_ce ON t(c, e)")
S += [f"INSERT INTO t(a, b, c) VALUES ({i}, 'b{i}', {i}.5)" for i in range(1, 30)]
S.append("INSERT INTO t(id, a, b) VALUES (100, 7, 'hundred')")
S.append("INSERT INTO t(b) VALUES ('onlyb')")
S.append("INSERT INTO t(a, b) VALUES (NULL, 'nullA')")
S.append("INSERT INTO t(a, b) VALUES (1, 'b1')")
S.append("INSERT OR IGNORE INTO t(a, b) VALUES (1, 'b1')")
S.append("INSERT OR REPLACE INTO t(a, b, c) VALUES (99, 'b2', 99.5)")
S.append("INSERT INTO t(id, b) VALUES (100, 'dup rowid')")
S.append("REPLACE INTO t(id, a, b) VALUES (100, 8, 'replaced100')")
S.append("INSERT INTO t(a, b, c, e) VALUES (3, 'x1', 1.5, 'x'), (3, 'x2', 2.5, 'y'), (4, 'x3', 1.5, 'x')")
S.append("INSERT INTO t(a, b, c, e) VALUES (3, 'x4', 1.5, 'x')")
S.append("SELECT id, a, b, c, e FROM t ORDER BY id")
S.append("UPDATE t SET a = a + 1 WHERE id < 10")
S.append("UPDATE t SET b = 'new' || id WHERE id BETWEEN 10 AND 12")
S.append("UPDATE t SET b = 'b1' WHERE id = 20")
S.append("UPDATE t SET id = id + 1000 WHERE id > 25 AND id < 100")
S.append("UPDATE OR REPLACE t SET b = 'b3' WHERE id = 21")
S.append("UPDATE t SET a = NULL WHERE id = 3")
S.append("UPDATE t SET c = c * 2, e = upper(e) WHERE a > 20")
S.append("SELECT id, a, b, c, e FROM t ORDER BY id")
S.append("DELETE FROM t WHERE id % 5 = 0")
S.append("DELETE FROM t WHERE b LIKE 'x%'")
S.append("SELECT count(*), sum(a), min(id), max(id) FROM t")
S.append("DELETE FROM t")
S.append("SELECT count(*) FROM t")
S += [f"INSERT INTO t(a, b) VALUES ({i}, 'again{i}')" for i in range(5)]
S.append("SELECT id, a, b FROM t ORDER BY id")
scenario("table with indexes", S, True)

S = ["CREATE TABLE s(k INTEGER PRIMARY KEY AUTOINCREMENT, v TEXT)",
     "INSERT INTO s(v) VALUES ('a'), ('b'), ('c')",
     "DELETE FROM s WHERE k = 3",
     "INSERT INTO s(v) VALUES ('d')",
     "INSERT INTO s(k, v) VALUES (50, 'e')",
     "INSERT INTO s(v) VALUES ('f')",
     "DELETE FROM s",
     "INSERT INTO s(v) VALUES ('g')",
     "SELECT k, v FROM s ORDER BY k",
     "SELECT name, seq FROM sqlite_sequence",
     "CREATE TABLE n(id INTEGER PRIMARY KEY, x)",
     "INSERT INTO n(x) VALUES (1), (2.5), ('three'), (x'04'), (NULL)",
     "INSERT INTO n VALUES (NULL, 'auto'), (-5, 'neg')",
     "INSERT INTO n(id, x) VALUES ('7', 'texty')",
     "INSERT INTO n(id, x) VALUES ('abc', 'bad')",
     "SELECT id, x, typeof(x) FROM n ORDER BY id",
     "DROP TABLE s",
     "SELECT name FROM sqlite_master ORDER BY name"]
scenario("autoincrement, rowid alias", S, True)

S = ["CREATE TABLE p(id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER CHECK (age >= 0), UNIQUE(name, age))",
     "INSERT INTO p VALUES (1, 'ann', 30), (2, 'bob', 25)",
     "INSERT INTO p VALUES (3, 'ann', 30)",
     "INSERT INTO p VALUES (3, 'cy', -1)",
     "INSERT INTO p VALUES (3, NULL, 5)",
     "INSERT INTO p VALUES (3, 'cy', NULL)",
     "INSERT INTO p VALUES (4, 'cy', NULL)",
     "UPDATE p SET age = -5 WHERE id = 1",
     "UPDATE p SET name = 'bob', age = 25 WHERE id = 1",
     "SELECT * FROM p ORDER BY id",
     "BEGIN",
     "INSERT INTO p VALUES (10, 'tx1', 1)",
     "INSERT INTO p VALUES (11, 'tx2', 2), (12, 'tx1', 1), (13, 'tx3', 3)",
     "SELECT count(*) FROM p",
     "INSERT INTO p VALUES (14, 'tx4', 4)",
     "ROLLBACK",
     "SELECT count(*) FROM p",
     "BEGIN",
     "DELETE FROM p WHERE id = 1",
     "INSERT INTO p VALUES (20, 'in tx', 9)",
     "COMMIT",
     "SELECT * FROM p ORDER BY id",
     "BEGIN",
     "CREATE TABLE q(a)",
     "INSERT INTO q VALUES (1)",
     "ROLLBACK",
     "SELECT name FROM sqlite_master ORDER BY name",
     "INSERT INTO nosuch VALUES (1)",
     "SELECT * FROM nosuch",
     "UPDATE p SET nosuch = 1",
     "INSERT INTO p(nosuch) VALUES (1)",
     "INSERT INTO p VALUES (1, 2)",
     "COMMIT",
     "ROLLBACK"]
scenario("constraints and transactions", S, True)

S = ["CREATE TABLE a(id INTEGER PRIMARY KEY, n INTEGER)",
     "CREATE TABLE b(id INTEGER PRIMARY KEY, a_id INTEGER, label TEXT)",
     "CREATE INDEX b_a ON b(a_id)"]
S += [f"INSERT INTO a VALUES ({i}, {i * 3 % 17})" for i in range(1, 60)]
S += [f"INSERT INTO b(a_id, label) VALUES ({random.randint(1, 70)}, 'l{i}')" for i in range(150)]
S += ["INSERT INTO a SELECT id + 100, n FROM a",
      "INSERT INTO b(a_id, label) SELECT id, 'copy' FROM a WHERE n > 10",
      "SELECT count(*) FROM a", "SELECT count(*) FROM b",
      "UPDATE b SET label = label || '!' WHERE a_id IN (SELECT id FROM a WHERE n < 3)",
      "UPDATE a SET n = (SELECT count(*) FROM b WHERE b.a_id = a.id)",
      "DELETE FROM b WHERE a_id NOT IN (SELECT id FROM a)",
      "DELETE FROM a WHERE id IN (SELECT a_id FROM b WHERE label LIKE '%!')",
      "SELECT a.id, a.n, count(b.id) FROM a LEFT JOIN b ON b.a_id = a.id GROUP BY a.id ORDER BY a.id",
      "CREATE TABLE c AS SELECT 1",
      "ALTER TABLE a ADD COLUMN extra TEXT DEFAULT 'dflt'",
      "ALTER TABLE a ADD COLUMN n2 INTEGER",
      "SELECT id, n, extra, n2 FROM a ORDER BY id LIMIT 5",
      "UPDATE a SET n2 = id WHERE id < 10",
      "SELECT id, extra, n2 FROM a WHERE id < 12 ORDER BY id",
      "INSERT INTO a(id, n) VALUES (1000, 1)",
      "SELECT extra FROM a WHERE id = 1000",
      "ALTER TABLE a ADD COLUMN bad INTEGER NOT NULL",
      "ALTER TABLE a ADD COLUMN good INTEGER NOT NULL DEFAULT 0",
      "DROP INDEX b_a",
      "DROP INDEX b_a",
      "DROP INDEX IF EXISTS b_a",
      "CREATE INDEX b_a2 ON b(a_id, label)",
      "SELECT count(*) FROM b WHERE a_id = 5",
      "DROP TABLE b",
      "DROP TABLE IF EXISTS b",
      "SELECT name, type FROM sqlite_master ORDER BY name"]
scenario("insert-select, subqueries, ALTER, DROP", S, True)

# upserts
S = ["CREATE TABLE u(k TEXT PRIMARY KEY, n INTEGER DEFAULT 0, note TEXT)",
     "INSERT INTO u(k, n) VALUES ('a', 1)",
     "INSERT INTO u(k, n) VALUES ('a', 5) ON CONFLICT(k) DO NOTHING",
     "INSERT INTO u(k, n) VALUES ('a', 5) ON CONFLICT(k) DO UPDATE SET n = n + excluded.n",
     "INSERT INTO u(k, n) VALUES ('b', 7) ON CONFLICT(k) DO UPDATE SET n = n + excluded.n",
     "INSERT INTO u(k, n, note) VALUES ('a', 1, 'x') ON CONFLICT(k) DO UPDATE SET note = excluded.note WHERE n > 100",
     "INSERT INTO u(k, n, note) VALUES ('a', 1, 'y') ON CONFLICT DO UPDATE SET note = excluded.note || '!'",
     "SELECT * FROM u ORDER BY k"]
scenario("upsert", S, True)

# a random workload
S = ["CREATE TABLE r(id INTEGER PRIMARY KEY, a INTEGER, b TEXT, c REAL)",
     "CREATE INDEX r_a ON r(a)", "CREATE UNIQUE INDEX r_b ON r(b)", "CREATE INDEX r_ac ON r(a, c)"]
words = ["x", "yy", "zzz", "w" * 40, "é", "", "long" * 30]
for i in range(1500):
    k = random.random()
    if k < 0.45:
        S.append(f"INSERT OR IGNORE INTO r(a, b, c) VALUES ({random.randint(0, 40)}, '{random.choice(words)}{random.randint(0, 600)}', {random.randint(0, 99) / 4})")
    elif k < 0.55:
        S.append(f"INSERT OR REPLACE INTO r(id, a, b, c) VALUES ({random.randint(1, 800)}, {random.randint(0, 40)}, '{random.choice(words)}{random.randint(0, 600)}', {random.randint(0, 99) / 4})")
    elif k < 0.75:
        S.append(f"UPDATE OR IGNORE r SET a = a + {random.randint(-3, 3)}, c = c + 1 WHERE a = {random.randint(0, 40)}")
    elif k < 0.85:
        S.append(f"UPDATE OR IGNORE r SET b = b || '{random.randint(0, 9)}' WHERE id = {random.randint(1, 800)}")
    elif k < 0.97:
        S.append(f"DELETE FROM r WHERE a = {random.randint(0, 40)} AND id % 3 = {random.randint(0, 2)}")
    else:
        S.append(f"DELETE FROM r WHERE id > {random.randint(300, 900)}")
    if i % 150 == 149:
        S.append("SELECT count(*), sum(a), sum(c), min(b), max(b) FROM r")
        S.append("SELECT a, count(*) FROM r GROUP BY a")
S.append("SELECT id, a, b, c FROM r ORDER BY id")
scenario("random workload", S, True)

print(f"{fails} differences")
sys.exit(1 if fails else 0)
