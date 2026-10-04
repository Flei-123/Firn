#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/check_crash.py <sql_probe> -- a process killed in the middle of a commit.
#
# 1. DETERMINISTIC: the probe kills itself (SIGKILL, in the middle of a page write when the
#    event is a write) at the n-th commit event, for n = 1, 2, 3 ... until the commit
#    finishes. After EVERY kill the file is opened by SQLite (which rolls the hot journal
#    back) and by lib/db (same), and the invariants must hold: money is conserved, the
#    log counts match, integrity_check is ok -- the transaction is all there or all gone.
# 2. KILLED FROM OUTSIDE: kill -9 at random moments of a long run of transactions.
import os, random, shutil, signal, sqlite3, subprocess, sys, tempfile, time

probe = sys.argv[1]
random.seed(7)
fails = 0

def mine(db, script_lines, crash=None, timeout=120):
    d = tempfile.mkdtemp()
    sc = os.path.join(d, "s.sql")
    with open(sc, "w") as f:
        f.write("\n".join(script_lines) + "\n")
    args = [probe, db, sc] + ([str(crash)] if crash else [])
    r = subprocess.run(args, capture_output=True, timeout=timeout)
    shutil.rmtree(d)
    return r

SETUP = ["PRAGMA cache_size = 64",
         "CREATE TABLE acct(id INTEGER PRIMARY KEY, bal INTEGER NOT NULL)",
         "CREATE INDEX acct_bal ON acct(bal)",
         "CREATE TABLE log(n INTEGER PRIMARY KEY, note TEXT)",
         "CREATE TABLE filler(k INTEGER PRIMARY KEY, pad TEXT)",
         "CREATE INDEX filler_pad ON filler(pad)",
         "CREATE TABLE meta(v INTEGER, filler INTEGER)",
         "INSERT INTO meta VALUES (0, 0)"]
SETUP += [f"INSERT INTO acct VALUES ({i}, 100)" for i in range(1, 51)]

def txn(i, big):
    s = ["PRAGMA cache_size = 64", "BEGIN"]
    for j in range(6):
        a = random.randint(1, 50)
        b = random.randint(1, 50)
        amt = random.randint(1, 30)
        s.append(f"UPDATE acct SET bal = bal - {amt} WHERE id = {a}")
        s.append(f"UPDATE acct SET bal = bal + {amt} WHERE id = {b}")
    s.append(f"INSERT INTO log(note) VALUES ('txn {i}')")
    s.append("UPDATE meta SET v = v + 1")
    if big:
        s.append(f"INSERT INTO filler(pad) SELECT '{'p' * 50}' || k FROM (SELECT 1 AS k UNION ALL SELECT 2 UNION ALL SELECT 3)")
        for k in range(40):
            s.append(f"INSERT INTO filler(pad) VALUES ('{'z' * 200}{i}_{k}')")
        s.append(f"UPDATE meta SET filler = filler + 43")
    s.append("COMMIT")
    return s

def check_invariants(path, label):
    global fails
    c = sqlite3.connect(path, timeout=30)
    try:
        ic = c.execute("PRAGMA integrity_check").fetchall()
        total = c.execute("SELECT sum(bal) FROM acct").fetchone()[0]
        v, fill = c.execute("SELECT v, filler FROM meta").fetchone()
        logs = c.execute("SELECT count(*) FROM log").fetchone()[0]
        fc = c.execute("SELECT count(*) FROM filler").fetchone()[0]
        ok = ic == [("ok",)] and total == 5000 and v == logs and fill == fc
        if not ok:
            fails += 1
            print(f"  FAIL {label}: integrity={ic[:2]} sum={total} v={v} logs={logs} filler={fill}/{fc}")
        return ok
    finally:
        c.close()

def check_mine(path, label):
    """lib/db recovers the same file (a copy) and agrees."""
    global fails
    out = mine(path, ["SELECT sum(bal) FROM acct", "SELECT v, filler FROM meta", "SELECT count(*) FROM log", "SELECT count(*) FROM filler", "PRAGMA integrity_check"])
    lines = out.stdout.decode().split("\n")
    rows = [l for l in lines if l.startswith("R ")]
    if len(rows) != 5 or rows[0] != "R I:5000" or rows[4] != "R T:" + b"ok".hex():
        fails += 1
        print(f"  FAIL {label}: lib/db sees {rows} {out.stderr.decode()[:100]}")
        return False
    v = int(rows[1].split("|")[0][4:]); fill = int(rows[1].split("|")[1][2:])
    logs = int(rows[2][4:]); fc = int(rows[3][4:])
    if v != logs or fill != fc:
        fails += 1
        print(f"  FAIL {label}: lib/db sees v={v} logs={logs} filler={fill}/{fc}")
        return False
    return True

with tempfile.TemporaryDirectory() as d:
    base = os.path.join(d, "base.db")
    r = mine(base, SETUP)
    assert r.returncode == 0, r.stderr
    # a few committed transactions first (small and big)
    for i in range(4):
        r = mine(base, txn(i, i % 2 == 1))
        assert r.returncode == 0, r.stderr
    committed = 4
    print("base database ready; deterministic crash points:")
    n = 1
    survived = 0
    rolled = 0
    outcomes = {}
    while n < 400:
        work = os.path.join(d, "work.db")
        for ext in ("", "-journal"):
            if os.path.exists(work + ext):
                os.unlink(work + ext)
        shutil.copy(base, work)
        big = (n % 3 != 0)
        script = txn(100 + n, big)
        r = mine(work, script, crash=n)
        killed = r.returncode == -9
        # recovery by SQLite on one copy, by lib/db on another
        w1 = os.path.join(d, "w1.db"); w2 = os.path.join(d, "w2.db")
        for ext in ("", "-journal"):
            if os.path.exists(w1 + ext): os.unlink(w1 + ext)
            if os.path.exists(w2 + ext): os.unlink(w2 + ext)
        shutil.copy(work, w1)
        if os.path.exists(work + "-journal"):
            shutil.copy(work + "-journal", w1 + "-journal")
            shutil.copy(work, w2)
            shutil.copy(work + "-journal", w2 + "-journal")
        else:
            shutil.copy(work, w2)
        ok1 = check_invariants(w1, f"point {n} (SQLite recovery)")
        ok2 = check_mine(w2, f"point {n} (lib/db recovery)")
        c = sqlite3.connect(w1)
        v = c.execute("SELECT v FROM meta").fetchone()[0]
        c.close()
        outcomes[v] = outcomes.get(v, 0) + 1
        if not killed:
            print(f"  point {n}: the commit finished before the kill point -> done")
            break
        n += 1
    print(f"  {n - 1} crash points checked; outcomes by meta.v: {outcomes} (4 = rolled back, 5 = committed)")
    # a transaction bigger than the page cache: pages are written to the database
    # file BEFORE the commit (after the journal is synced); any kill must still roll back
    print("transaction larger than the cache (pages spilled before the commit):")
    big_txn = ["PRAGMA cache_size = 64", "BEGIN"]
    for i in range(0, 1800, 150):
        big_txn.append("INSERT INTO filler(pad) SELECT 'q' || (k + %d) || '%s' FROM (%s)" % (
            i, "x" * 300, " UNION ALL ".join(f"SELECT {j} AS k" for j in range(150))))
    big_txn += ["UPDATE meta SET v = v + 1, filler = filler + 1800", "INSERT INTO log(note) VALUES ('big')", "COMMIT"]
    work = os.path.join(d, "big.db")
    for ext in ("", "-journal"):
        if os.path.exists(work + ext):
            os.unlink(work + ext)
    shutil.copy(base, work)
    t0 = time.time()
    r = mine(work, big_txn)
    assert r.returncode == 0, r.stderr
    check_invariants(work, "big transaction without a kill")
    print(f"  the big transaction itself works ({time.time() - t0:.1f} s)")
    points = sorted(set([1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144, 233, 377, 610, 987, 1500, 2000] + [random.randint(1, 1500) for _ in range(25)]))
    spilled_back = 0
    for n in points:
        for ext in ("", "-journal"):
            if os.path.exists(work + ext):
                os.unlink(work + ext)
        shutil.copy(base, work)
        r = mine(work, big_txn, crash=n)
        killed = r.returncode == -9
        w1 = os.path.join(d, "b1.db"); w2 = os.path.join(d, "b2.db")
        for ext in ("", "-journal"):
            for w in (w1, w2):
                if os.path.exists(w + ext): os.unlink(w + ext)
        shutil.copy(work, w1); shutil.copy(work, w2)
        if os.path.exists(work + "-journal"):
            shutil.copy(work + "-journal", w1 + "-journal"); shutil.copy(work + "-journal", w2 + "-journal")
        ok1 = check_invariants(w1, f"big txn point {n} (SQLite recovery)")
        ok2 = check_mine(w2, f"big txn point {n} (lib/db recovery)")
        if killed and ok1 and ok2:
            spilled_back += 1
        if not killed:
            print(f"  point {n}: the commit finished before the kill point")
            break
    print(f"  {spilled_back} kills inside the big commit, every one rolled back cleanly")
    # killed from outside at random moments
    print("random kill -9 during a stream of transactions:")
    work = os.path.join(d, "ext.db")
    shutil.copy(base, work)
    kills = 0
    for round_ in range(25):
        script = []
        for i in range(60):
            script += txn(1000 + round_ * 100 + i, i % 5 == 0)[1:]
            # (the PRAGMA line only once)
        script = ["PRAGMA cache_size = 64"] + script
        dd = tempfile.mkdtemp()
        sc = os.path.join(dd, "s.sql")
        open(sc, "w").write("\n".join(script) + "\n")
        p = subprocess.Popen([probe, work, sc], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        time.sleep(random.uniform(0.02, 0.35))
        if p.poll() is None:
            p.send_signal(signal.SIGKILL)
            kills += 1
        p.wait()
        shutil.rmtree(dd)
        w1 = os.path.join(d, "x1.db")
        for ext in ("", "-journal"):
            if os.path.exists(w1 + ext): os.unlink(w1 + ext)
        shutil.copy(work, w1)
        if os.path.exists(work + "-journal"):
            shutil.copy(work + "-journal", w1 + "-journal")
        if round_ % 2 == 0:
            check_invariants(w1, f"external kill {round_} (SQLite recovery)")
            # lib/db then recovers the REAL file and goes on
            check_mine(work, f"external kill {round_} (lib/db recovery)")
        else:
            check_mine(work, f"external kill {round_} (lib/db recovery)")
            check_invariants(work, f"external kill {round_} (SQLite after lib/db)")
    print(f"  {kills} processes killed")
print("FAILED" if fails else "ALL OK", fails)
sys.exit(1 if fails else 0)
