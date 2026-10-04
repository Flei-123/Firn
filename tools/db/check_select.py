#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/check_select.py <sql_probe> -- SELECT of lib/db against SQLite on a database
# SQLite made: joins (inner, left, using, comma, self, three-way, index and rowid lookups),
# aggregates, GROUP BY / HAVING, DISTINCT, ORDER BY / LIMIT / OFFSET, sub-selects
# (IN, EXISTS, scalar, correlated, FROM), compound selects, CASE, functions.
# A line starting with '!' compares the rows IN ORDER, others as a multiset.
import os, random, shutil, sqlite3, struct, subprocess, sys, tempfile

probe = sys.argv[1]
random.seed(20261004)

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

def make_db(path):
    c = sqlite3.connect(path)
    c.executescript("""
    CREATE TABLE t1(id INTEGER PRIMARY KEY, a INTEGER, b TEXT, c REAL, d BLOB);
    CREATE TABLE t2(id INTEGER PRIMARY KEY, t1_id INTEGER, label TEXT, score REAL);
    CREATE TABLE t3(x TEXT, y INTEGER, z INTEGER);
    CREATE TABLE t4(k INTEGER PRIMARY KEY, grp TEXT, v INTEGER);
    CREATE INDEX t2_t1 ON t2(t1_id);
    CREATE INDEX t3_xy ON t3(x, y);
    CREATE UNIQUE INDEX t3_z ON t3(z);
    CREATE INDEX t1_a ON t1(a);
    CREATE INDEX t1_b ON t1(b DESC);
    """)
    words = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa", "lambda", "mu", "Alpha", "BETA", "", "日本", "ünï"]
    for i in range(1, 241):
        a = random.choice([None, random.randint(-50, 50), random.randint(0, 5)])
        b = random.choice([None, random.choice(words), random.choice(words) + str(random.randint(0, 9))])
        cc = random.choice([None, round(random.uniform(-100, 100), 3), float(random.randint(-5, 5))])
        d = random.choice([None, bytes(random.randint(0, 255) for _ in range(random.randint(0, 6)))])
        c.execute("INSERT INTO t1 VALUES (?,?,?,?,?)", (i, a, b, cc, d))
    for i in range(1, 401):
        t1 = random.choice([None, random.randint(1, 260)])
        c.execute("INSERT INTO t2 VALUES (?,?,?,?)", (i, t1, random.choice(words) + str(i % 7), random.choice([None, random.random() * 10])))
    zs = random.sample(range(1, 1000), 150)
    for i, z in enumerate(zs):
        c.execute("INSERT INTO t3 VALUES (?,?,?)", (random.choice([None, random.choice(words)]), random.choice([None, random.randint(0, 9)]), z))
    for i in range(1, 61):
        c.execute("INSERT INTO t4 VALUES (?,?,?)", (i * 3, random.choice(["g1", "g2", "g3", None]), random.choice([None, random.randint(1, 20)])))
    c.commit()
    c.close()

Q = []
def q(sql, exact=False):
    Q.append(("!" if exact else "") + sql)

q("SELECT * FROM t1 ORDER BY id", True)
q("SELECT id, a, b FROM t1 WHERE a > 10")
q("SELECT id FROM t1 WHERE a = 3 OR b = 'alpha'")
q("SELECT id, a * 2 + 1, b || '!' FROM t1 WHERE a IS NOT NULL AND b IS NOT NULL")
q("SELECT count(*), count(a), count(b), count(c), count(d), sum(a), total(a), avg(a), min(a), max(a), min(b), max(b), sum(c), avg(c) FROM t1")
q("SELECT count(*) FROM t1 WHERE a > 1000")
q("SELECT sum(a), avg(a), min(a), max(a), total(a) FROM t1 WHERE a > 1000")
q("SELECT a, count(*), sum(c), min(b), max(id) FROM t1 GROUP BY a")
q("SELECT b, count(*) c FROM t1 GROUP BY b HAVING count(*) > 2")
q("SELECT a % 3, count(*), group_concat(b) FROM t1 WHERE id < 4 GROUP BY a % 3")
q("SELECT DISTINCT a FROM t1")
q("SELECT DISTINCT a, b FROM t1 WHERE id < 60")
q("SELECT count(DISTINCT a), count(DISTINCT b), sum(DISTINCT a) FROM t1")
q("SELECT id, a FROM t1 ORDER BY a, id", True)
q("SELECT id, a FROM t1 ORDER BY a DESC, id DESC", True)
q("SELECT id, b FROM t1 ORDER BY b, id LIMIT 20", True)
q("SELECT id, b FROM t1 ORDER BY b DESC, id LIMIT 20 OFFSET 5", True)
q("SELECT id FROM t1 ORDER BY id LIMIT 5, 3", True)
q("SELECT id, c FROM t1 ORDER BY c NULLS LAST, id LIMIT 10", True)
q("SELECT id FROM t1 ORDER BY 1 DESC LIMIT 7", True)
q("SELECT a AS x, id FROM t1 ORDER BY x, id", True)
q("SELECT id, a + id AS s FROM t1 ORDER BY s DESC, id LIMIT 15", True)
q("SELECT id FROM t1 WHERE a BETWEEN 2 AND 4")
q("SELECT id FROM t1 WHERE a NOT BETWEEN 2 AND 4")
q("SELECT id FROM t1 WHERE a IN (1, 2, 3) OR b IN ('alpha', 'beta')")
q("SELECT id FROM t1 WHERE b LIKE 'a%' OR b LIKE '%A'")
q("SELECT id FROM t1 WHERE b GLOB '[a-c]*'")
q("SELECT id FROM t1 WHERE c > 0.5 AND c < 50")
q("SELECT id FROM t1 WHERE id = 17")
q("SELECT id FROM t1 WHERE id = 17.0")
q("SELECT id FROM t1 WHERE id = '17'")
q("SELECT id FROM t1 WHERE id > 100 AND id <= 120")
q("SELECT id FROM t1 WHERE id >= 230")
q("SELECT id FROM t1 WHERE id < 5")
q("SELECT id FROM t1 WHERE id > 2.5 AND id < 6.5")
q("SELECT id FROM t1 WHERE 100 < id AND 110 >= id")
q("SELECT id FROM t1 WHERE a = 3")
q("SELECT id FROM t1 WHERE a > 3 AND a < 8")
q("SELECT id FROM t1 WHERE a >= 49")
q("SELECT id FROM t1 WHERE a < -45")
q("SELECT id FROM t1 WHERE a = '3'")
q("SELECT id FROM t1 WHERE b = 'alpha'")
q("SELECT id FROM t1 WHERE b > 'm'")
q("SELECT id FROM t1 WHERE b >= 'gamma' AND b < 'zeta'")
q("SELECT id FROM t1 WHERE b = 5")
q("SELECT t2.id, t2.label, t1.b FROM t2 JOIN t1 ON t2.t1_id = t1.id WHERE t2.id < 60")
q("SELECT t2.id, t1.a FROM t2, t1 WHERE t2.t1_id = t1.id AND t1.a > 0")
q("SELECT t1.id, t2.id FROM t1 LEFT JOIN t2 ON t2.t1_id = t1.id WHERE t1.id < 30")
q("SELECT t1.id, count(t2.id) FROM t1 LEFT JOIN t2 ON t2.t1_id = t1.id GROUP BY t1.id HAVING count(t2.id) = 0")
q("SELECT t1.id, t2.id FROM t1 LEFT JOIN t2 ON t2.t1_id = t1.id AND t2.score > 5 WHERE t1.id < 40")
q("SELECT t1.id, t2.id FROM t1 LEFT JOIN t2 ON t2.t1_id = t1.id WHERE t2.id IS NULL AND t1.id < 100")
q("SELECT a.id, b.id FROM t1 a JOIN t1 b ON a.a = b.a WHERE a.id < b.id AND a.id < 30")
q("SELECT t1.id, t2.id, t4.k FROM t1 JOIN t2 ON t2.t1_id = t1.id JOIN t4 ON t4.k = t1.id WHERE t1.id < 100")
q("SELECT t1.id, t2.label FROM t1 JOIN t2 USING (id) WHERE t1.id < 50")
q("SELECT * FROM t1 JOIN t4 ON t1.id = t4.k WHERE t4.grp = 'g1'")
q("SELECT t3.x, t3.y, t3.z FROM t3 WHERE x = 'alpha' AND y = 3")
q("SELECT z FROM t3 WHERE x = 'beta'")
q("SELECT z FROM t3 WHERE x = 'beta' AND y > 2")
q("SELECT z FROM t3 WHERE z = 500")
q("SELECT z FROM t3 WHERE z > 900 OR z < 20")
q("SELECT x, count(*), min(z) FROM t3 GROUP BY x")
q("SELECT id FROM t1 WHERE id IN (SELECT t1_id FROM t2 WHERE score > 8)")
q("SELECT id FROM t1 WHERE id NOT IN (SELECT t1_id FROM t2 WHERE t1_id IS NOT NULL)")
q("SELECT id FROM t1 WHERE EXISTS (SELECT 1 FROM t2 WHERE t2.t1_id = t1.id AND t2.score > 9)")
q("SELECT id FROM t1 WHERE NOT EXISTS (SELECT 1 FROM t2 WHERE t2.t1_id = t1.id)")
q("SELECT id, (SELECT count(*) FROM t2 WHERE t2.t1_id = t1.id) FROM t1 WHERE id < 30")
q("SELECT id, (SELECT max(score) FROM t2 WHERE t2.t1_id = t1.id) FROM t1 WHERE id < 30")
q("SELECT (SELECT max(id) FROM t1), (SELECT min(a) FROM t1)")
q("SELECT id FROM t1 WHERE a > (SELECT avg(a) FROM t1)")
q("SELECT s.a, s.n FROM (SELECT a, count(*) n FROM t1 GROUP BY a) s WHERE s.n > 3")
q("SELECT x.id FROM (SELECT id FROM t1 WHERE a > 40) x JOIN t2 ON t2.t1_id = x.id")
q("SELECT a FROM t1 WHERE a < 3 UNION SELECT a FROM t1 WHERE a > 45")
q("SELECT a FROM t1 WHERE a < 3 UNION ALL SELECT a FROM t1 WHERE a < 3")
q("SELECT a FROM t1 WHERE a BETWEEN 0 AND 5 INTERSECT SELECT a FROM t1 WHERE a BETWEEN 3 AND 8")
q("SELECT a FROM t1 WHERE a BETWEEN 0 AND 5 EXCEPT SELECT a FROM t1 WHERE a BETWEEN 3 AND 8")
q("SELECT id, a FROM t1 WHERE id < 5 UNION ALL SELECT id, a FROM t1 WHERE id > 236 ORDER BY 1 DESC", True)
q("SELECT b FROM t1 UNION SELECT label FROM t2 ORDER BY 1", True)
q("SELECT id, CASE WHEN a > 10 THEN 'big' WHEN a > 0 THEN 'small' WHEN a IS NULL THEN 'null' ELSE 'neg' END FROM t1")
q("SELECT id, CASE a WHEN 1 THEN 'one' WHEN 2 THEN 'two' END FROM t1")
q("SELECT upper(b), length(b), substr(b, 2, 3), abs(a), round(c, 1), coalesce(a, -1), typeof(c), hex(d) FROM t1 WHERE id < 40")
q("SELECT id FROM t1 WHERE length(b) > 5 AND instr(b, 'a') > 0")
q("SELECT a, sum(c) s FROM t1 GROUP BY a HAVING s > 0 ORDER BY a", True)
q("SELECT a, count(*) FROM t1 GROUP BY a ORDER BY count(*) DESC, a", True)
q("SELECT b, a FROM t1 GROUP BY b, a")
q("SELECT count(*) FROM t1 a, t1 b WHERE a.a = b.a")
q("SELECT count(*) FROM t2 WHERE t1_id IN (1, 2, 3, 4, 5)")
q("SELECT min(id), max(id), count(*) FROM t2 WHERE t1_id = 7")
q("SELECT 1 + 1, 'x' || 'y', NULL")
q("SELECT 1 WHERE 1 = 2")
q("SELECT * FROM t4 ORDER BY k", True)
q("SELECT * FROM t4 WHERE grp IS NULL")
q("SELECT grp, sum(v), count(v), avg(v) FROM t4 GROUP BY grp ORDER BY grp", True)
q("SELECT k FROM t4 WHERE v IN (1, 5, 10, 15, 20) ORDER BY k LIMIT 3", True)
q("SELECT t4.k, t1.b FROM t4 LEFT JOIN t1 ON t1.id = t4.k * 2 WHERE t4.k < 40")
q("SELECT * FROM sqlite_master WHERE type = 'table' ORDER BY name", True)
q("SELECT name FROM sqlite_master WHERE type = 'index' ORDER BY name", True)
q("SELECT count(*) FROM sqlite_master")
q("SELECT t1.id FROM t1 WHERE t1.a = (SELECT t1b.a FROM t1 t1b WHERE t1b.id = 5)")
q("SELECT id, (SELECT group_concat(label) FROM t2 WHERE t1_id = 1) FROM t1 WHERE id = 1")
q("SELECT DISTINCT t1_id FROM t2 WHERE t1_id < 20 ORDER BY t1_id", True)
q("SELECT a, b FROM t1 WHERE id IN (3, 4, 5) ORDER BY id DESC", True)
q("SELECT id FROM t1 WHERE id IN (SELECT k FROM t4 WHERE v > 10)")
q("SELECT id FROM t1 WHERE (a > 40 OR a < -40) AND b IS NOT NULL ORDER BY id", True)
q("SELECT count(*), a IS NULL FROM t1 GROUP BY a IS NULL")
q("SELECT lower(b) l, count(*) FROM t1 GROUP BY lower(b) ORDER BY l", True)
q("SELECT 5 % 3, -7 / 2, 7.5 / 2, 2 * 3.5")

with tempfile.TemporaryDirectory() as d:
    path = os.path.join(d, "s.db")
    make_db(path)
    script = os.path.join(d, "script.sql")
    with open(script, "w", encoding="utf-8") as f:
        for line in Q:
            f.write(line[1:] if line.startswith("!") else line)
            f.write("\n")
    work = os.path.join(d, "work.db")
    shutil.copy(path, work)
    r = subprocess.run([probe, work, script], capture_output=True)
    lines = r.stdout.decode("utf-8", "replace").split("\n")
    if r.returncode != 0:
        print("probe exit", r.returncode, r.stderr.decode()[:300])
    ref = sqlite3.connect(path)
    fails = 0
    pos = 0
    for line in Q:
        exact = line.startswith("!")
        sql = line[1:] if exact else line
        rows = []
        while pos < len(lines) and (lines[pos].startswith("R ")):
            rows.append(lines[pos][2:])
            pos += 1
        status = lines[pos] if pos < len(lines) else "?"
        pos += 1
        try:
            ref_rows = [("|".join(enc(v) for v in row)) for row in ref.execute(sql).fetchall()]
            ref_status = "OK"
        except sqlite3.Error as e:
            ref_rows = []
            ref_status = "E"
        mine_status = "E" if status.startswith("E") else status
        if ref_status != mine_status:
            fails += 1
            print(f"STATUS DIFF {sql!r}: sqlite={ref_status} mine={status}")
            continue
        a = rows if exact else sorted(rows)
        b = ref_rows if exact else sorted(ref_rows)
        if a != b:
            fails += 1
            print(f"ROWS DIFF {sql!r}: mine {len(a)} rows, sqlite {len(b)} rows")
            for x, y in zip(a, b):
                if x != y:
                    print("   first difference: mine", x[:200], " sqlite", y[:200])
                    break
    print(f"{len(Q)} queries, {fails} differences")
    sys.exit(1 if fails else 0)
