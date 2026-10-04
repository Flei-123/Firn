p = '/root/firn-wt-db/tools/db/check_crash.py'
s = open(p).read()
old = '''    # killed from outside at random moments'''
new = '''    # a transaction bigger than the page cache: pages are written to the database
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
    # killed from outside at random moments'''
assert old in s
s = s.replace(old, new, 1)
open(p, 'w').write(s)
print('ok22')
