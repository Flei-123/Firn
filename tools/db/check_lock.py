#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/check_lock.py <sql_probe> -- locks between lib/db and SQLite on one file.
#
# lib/db uses SQLite's own lock bytes (SHARED / RESERVED / PENDING / EXCLUSIVE), so a
# process of either kind excludes the other:
#   * a writer in a transaction does not stop readers, but stops other writers (BUSY)
#   * a commit waits for the readers to finish (BUSY after the timeout, the transaction
#     stays open and can be retried)
# and then a stress run: several processes of both kinds add to a counter; nothing is lost.
import os, shutil, sqlite3, subprocess, sys, tempfile, threading, time

probe = sys.argv[1]
fails = 0

def expect(label, cond, extra=""):
    global fails
    if not cond:
        fails += 1
        print(f"  FAIL {label} {extra}")
    else:
        print(f"  ok   {label}")

def spawn(db, lines):
    d = tempfile.mkdtemp()
    sc = os.path.join(d, "s.sql")
    with open(sc, "w") as f:
        f.write("\n".join(lines) + "\n")
    p = subprocess.Popen([probe, db, sc], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, bufsize=1)
    return p, d

def read_until(p, token, timeout=10):
    t0 = time.time()
    out = []
    while time.time() - t0 < timeout:
        line = p.stdout.readline()
        if not line:
            break
        out.append(line.strip())
        if token in line:
            return out
    return out

with tempfile.TemporaryDirectory() as d:
    db = os.path.join(d, "l.db")
    c = sqlite3.connect(db)
    c.execute("CREATE TABLE k(id INTEGER PRIMARY KEY, n INTEGER)")
    c.execute("INSERT INTO k VALUES (1, 0)")
    c.commit()
    c.close()

    print("lib/db holds a write transaction:")
    p, tmp = spawn(db, ["BEGIN IMMEDIATE", "UPDATE k SET n = n + 1", ".echo LOCKED", ".wait", "COMMIT", ".echo COMMITTED"])
    out = read_until(p, "LOCKED")
    expect("writer reached its locked state", "LOCKED" in " ".join(out), str(out))
    r = sqlite3.connect(db, timeout=0.2)
    try:
        v = r.execute("SELECT n FROM k").fetchone()[0]
        expect("SQLite can read while lib/db has RESERVED", v == 0, str(v))
    except sqlite3.Error as e:
        expect("SQLite can read while lib/db has RESERVED", False, str(e))
    try:
        r.execute("BEGIN IMMEDIATE")
        expect("SQLite cannot write while lib/db has RESERVED", False)
        r.rollback()
    except sqlite3.OperationalError as e:
        expect("SQLite cannot write while lib/db has RESERVED", "locked" in str(e), str(e))
    r.close()
    p.stdin.write("go\n")
    p.stdin.flush()
    out2 = read_until(p, "COMMITTED")
    p.wait()
    shutil.rmtree(tmp)
    r = sqlite3.connect(db)
    expect("the commit of lib/db is visible to SQLite", r.execute("SELECT n FROM k").fetchone()[0] == 1)
    r.close()

    print("SQLite holds a write transaction:")
    s = sqlite3.connect(db, isolation_level=None)
    s.execute("BEGIN IMMEDIATE")
    s.execute("UPDATE k SET n = 100")
    p, tmp = spawn(db, [".busy 300", "SELECT n FROM k", "UPDATE k SET n = n + 1", "SELECT n FROM k"])
    out, err = p.communicate(timeout=20)
    lines = out.strip().split("\n")
    expect("lib/db reads while SQLite has RESERVED", lines[0] == "R I:1", str(lines))
    expect("lib/db gets BUSY on a write", len(lines) > 2 and "locked" in lines[2], str(lines))
    s.execute("COMMIT")
    s.close()
    shutil.rmtree(tmp)

    print("SQLite holds a read transaction while lib/db commits:")
    s = sqlite3.connect(db, isolation_level=None)
    s.execute("BEGIN")
    s.execute("SELECT n FROM k").fetchall()
    p, tmp = spawn(db, [".busy 200", "BEGIN", "UPDATE k SET n = 7", "COMMIT", ".echo AFTER"])
    out, err = p.communicate(timeout=20)
    lines = out.strip().split("\n")
    expect("lib/db commit answers BUSY while a reader holds SHARED", any("locked" in l for l in lines), str(lines))
    s.execute("COMMIT")
    s.close()
    shutil.rmtree(tmp)

    print("stress: lib/db and SQLite processes add to one counter:")
    db2 = os.path.join(d, "stress.db")
    c = sqlite3.connect(db2)
    c.execute("CREATE TABLE ctr(id INTEGER PRIMARY KEY, n INTEGER)")
    c.execute("INSERT INTO ctr VALUES (1, 0)")
    c.commit()
    c.close()
    N_MINE, N_SQLITE, PER = 3, 3, 60
    results = []
    def sqlite_worker():
        done = 0
        con = sqlite3.connect(db2, timeout=30, isolation_level=None)
        for _ in range(PER):
            while True:
                try:
                    con.execute("BEGIN IMMEDIATE")
                    con.execute("UPDATE ctr SET n = n + 1 WHERE id = 1")
                    con.execute("COMMIT")
                    done += 1
                    break
                except sqlite3.OperationalError:
                    try:
                        con.execute("ROLLBACK")
                    except sqlite3.Error:
                        pass
                    time.sleep(0.001)
        con.close()
        results.append(done)
    procs = []
    for i in range(N_MINE):
        sc = tempfile.mkdtemp()
        f = os.path.join(sc, "s.sql")
        with open(f, "w") as fh:
            # each statement is its own transaction; a busy answer is retried by the script's author:
            # here the busy timeout is long enough
            fh.write(".busy 30000\n" + "\n".join(["UPDATE ctr SET n = n + 1 WHERE id = 1"] * PER) + "\n")
        procs.append((subprocess.Popen([probe, db2, f], stdout=subprocess.PIPE, stderr=subprocess.PIPE), sc))
    threads = [threading.Thread(target=sqlite_worker) for _ in range(N_SQLITE)]
    for t in threads:
        t.start()
    ok_mine = 0
    for pr, sc in procs:
        out, err = pr.communicate(timeout=300)
        ok_mine += out.decode().count("OK\n")
        shutil.rmtree(sc)
    for t in threads:
        t.join()
    c = sqlite3.connect(db2)
    n = c.execute("SELECT n FROM ctr").fetchone()[0]
    ic = c.execute("PRAGMA integrity_check").fetchall()
    c.close()
    expect(f"no update lost: counter {n} == {ok_mine} (lib/db) + {sum(results)} (SQLite)", n == ok_mine + sum(results), f"ok_mine={ok_mine}")
    expect("integrity_check after the stress", ic == [("ok",)], str(ic))
print("FAILED" if fails else "ALL OK", fails)
sys.exit(1 if fails else 0)
