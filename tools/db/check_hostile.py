#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/check_hostile.py <sql_probe> [rounds] -- damaged and hostile database files.
#
# Valid databases (made by SQLite: several page sizes, overflow pages, indexes, a freelist)
# are damaged in many ways -- flipped bytes, pages overwritten with random data or other
# pages (cycles!), header fields set to extremes, cell pointers pointed anywhere, files
# cut short or extended, pointers to pages out of range -- and lib/db reads AND writes them.
# What must hold: it never crashes (signal), never panics (an arithmetic trap), never hangs
# (timeout), never uses much memory; it answers an error or an answer. This is not about
# the answer being right: the file is wrong.
import multiprocessing, os, random, shutil, sqlite3, struct, subprocess, sys, tempfile
try:
    import resource  # POSIX only; on Windows no memory limit is set
except ImportError:
    resource = None

probe = sys.argv[1]
ROUNDS = int(sys.argv[2]) if len(sys.argv) > 2 else 400
RUNNER = os.environ.get("RUNNER", "").split()

QUERIES = ["SELECT count(*) FROM t", "SELECT * FROM t ORDER BY id LIMIT 50", "SELECT count(*) FROM t WHERE b LIKE '%a%'",
           "SELECT a, count(*) FROM t GROUP BY a", "SELECT * FROM t WHERE a = 5", "SELECT * FROM t WHERE b > 'm' ORDER BY b",
           "SELECT length(c) FROM t WHERE id % 7 = 0", "SELECT count(*) FROM u", "SELECT t.id, u.v FROM t JOIN u ON u.t_id = t.id LIMIT 30",
           "SELECT * FROM sqlite_master", "PRAGMA integrity_check", "PRAGMA table_info(t)",
           "INSERT INTO t(a, b, c) VALUES (1, 'new', 'zz')", "UPDATE t SET a = a + 1 WHERE id % 5 = 0", "DELETE FROM t WHERE id % 3 = 0",
           "INSERT INTO u(t_id, v) SELECT id, a FROM t", "CREATE INDEX t_c ON t(c)", "DROP INDEX t_a", "SELECT count(*) FROM t",
           "PRAGMA integrity_check"]

def make_dbs(d):
    paths = []
    for ps, n in [(512, 400), (1024, 600), (4096, 900), (4096, 60)]:
        p = os.path.join(d, f"v{ps}_{n}.db")
        c = sqlite3.connect(p)
        c.execute(f"PRAGMA page_size={ps}")
        c.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER, b TEXT, c BLOB)")
        c.execute("CREATE TABLE u(id INTEGER PRIMARY KEY, t_id INTEGER, v INTEGER)")
        c.execute("CREATE INDEX t_a ON t(a)")
        c.execute("CREATE INDEX t_b ON t(b)")
        c.execute("CREATE UNIQUE INDEX u_t ON u(t_id, v)")
        rnd = random.Random(ps + n)
        for i in range(1, n + 1):
            blob = bytes(rnd.randint(0, 255) for _ in range(rnd.choice([0, 3, 40, 700, 2500]))) if rnd.random() < 0.3 else None
            c.execute("INSERT INTO t VALUES (?,?,?,?)", (i, rnd.randint(0, 20), "w" * rnd.randint(0, 30) + str(i), blob))
            if i % 3 == 0:
                c.execute("INSERT INTO u(t_id, v) VALUES (?, ?)", (i, rnd.randint(0, 50)))
        c.commit()
        c.execute("DELETE FROM t WHERE id % 11 = 0")
        c.commit()
        c.close()
        paths.append(p)
    return paths

def mutate(data, rnd):
    data = bytearray(data)
    ps = struct.unpack(">H", data[16:18])[0]
    if ps == 1:
        ps = 65536
    npages = max(1, len(data) // ps)
    kind = rnd.randint(0, 11)
    if kind == 0:  # byte flips anywhere
        for _ in range(rnd.randint(1, 20)):
            data[rnd.randrange(len(data))] = rnd.randint(0, 255)
    elif kind == 1:  # byte flips near the page headers and cell pointer arrays
        for _ in range(rnd.randint(1, 30)):
            pg = rnd.randrange(npages)
            off = pg * ps + rnd.randint(0, min(60, ps - 1)) + (100 if pg == 0 and rnd.random() < 0.5 else 0)
            if off < len(data):
                data[off] = rnd.randint(0, 255)
    elif kind == 2:  # a page of random bytes
        pg = rnd.randrange(npages)
        data[pg * ps:(pg + 1) * ps] = bytes(rnd.randint(0, 255) for _ in range(ps))[: len(data[pg * ps:(pg + 1) * ps])]
    elif kind == 3:  # one page copied over another (cycles, shared children)
        a = rnd.randrange(npages); b = rnd.randrange(npages)
        data[a * ps:(a + 1) * ps] = data[b * ps:(b + 1) * ps]
    elif kind == 4:  # header fields to extremes
        for off, width in [(16, 2), (18, 1), (19, 1), (20, 1), (24, 4), (28, 4), (32, 4), (36, 4), (40, 4), (44, 4), (52, 4), (56, 4), (92, 4)]:
            if rnd.random() < 0.25:
                v = rnd.choice([0, 1, 2, 3, 255, 65535, 4294967295, 2 ** 31, 2 ** 31 - 1, rnd.randrange(2 ** 32)]) & ((1 << (8 * width)) - 1)
                data[off:off + width] = v.to_bytes(width, "big")
    elif kind == 5:  # truncated
        data = data[: rnd.randrange(len(data))]
    elif kind == 6:  # extended with garbage
        data += bytes(rnd.randint(0, 255) for _ in range(rnd.randint(1, 3 * ps)))
    elif kind == 7:  # cell pointers to anywhere
        pg = rnd.randrange(npages)
        base = pg * ps + (100 if pg == 0 else 0)
        for _ in range(rnd.randint(1, 6)):
            off = base + 8 + 2 * rnd.randint(0, 40)
            if off + 2 < len(data):
                data[off:off + 2] = rnd.choice([0, 1, 4, ps - 1, ps, ps + 1, 65535, rnd.randrange(65536)]).to_bytes(2, "big")
    elif kind == 8:  # child page numbers
        pg = rnd.randrange(npages)
        base = pg * ps + (100 if pg == 0 else 0)
        if data[base] in (2, 5):
            for off in range(base + 12, base + 12 + 40, 2):
                if rnd.random() < 0.2 and off + 4 < len(data):
                    data[off:off + 4] = rnd.choice([0, 1, 2, pg + 1, npages + 1, 4294967295, rnd.randrange(npages + 3)]).to_bytes(4, "big")
            data[base + 8:base + 12] = rnd.choice([0, pg + 1, 1, npages + 5, rnd.randrange(npages + 1)]).to_bytes(4, "big")
    elif kind == 9:  # an overflow chain or freelist trunk loop
        pg = rnd.randrange(1, npages)
        data[pg * ps:pg * ps + 4] = (pg + 1).to_bytes(4, "big")
    elif kind == 10:  # the type byte of pages
        for _ in range(rnd.randint(1, 5)):
            pg = rnd.randrange(npages)
            off = pg * ps + (100 if pg == 0 else 0)
            data[off] = rnd.choice([0, 1, 2, 5, 10, 13, 255])
    else:  # page 1 zeroed or the sqlite_master tree damaged
        data[100:200] = bytes(rnd.randint(0, 255) for _ in range(100))
    return bytes(data)

def limit():
    resource.setrlimit(resource.RLIMIT_AS, (3 * 1024 ** 3, 3 * 1024 ** 3))

def one(args):
    src, seed, qpath = args
    rnd = random.Random(seed)
    data = open(src, "rb").read()
    d = tempfile.mkdtemp()
    try:
        p = os.path.join(d, "h.db")
        open(p, "wb").write(mutate(data, rnd))
        try:
            r = subprocess.run(RUNNER + [probe, p, qpath], capture_output=True, timeout=30,
                               **({"preexec_fn": limit} if resource else {}))
        except subprocess.TimeoutExpired:
            return (seed, src, "TIMEOUT", "")
        out = r.stdout.decode("utf-8", "replace")
        err = r.stderr.decode("utf-8", "replace")
        if r.returncode < 0:
            return (seed, src, f"SIGNAL {-r.returncode}", err[:200])
        if "panic" in err or "panic" in out:
            return (seed, src, "PANIC", (err + out)[:300])
        if r.returncode not in (0, 1):
            return (seed, src, f"EXIT {r.returncode}", err[:200])
        # SQLite must not crash on the same file either way round? (not required)
        return (seed, src, "ok", "E" if "\nE " in out or out.startswith("E ") else "")
    finally:
        shutil.rmtree(d, ignore_errors=True)

if __name__ == "__main__":
    with tempfile.TemporaryDirectory() as d:
        paths = make_dbs(d)
        qpath = os.path.join(d, "q.sql")
        open(qpath, "w").write("\n".join(QUERIES) + "\n")
        jobs = [(paths[i % len(paths)], 1000 + i, qpath) for i in range(ROUNDS)]
        bad = []
        errs = 0
        with multiprocessing.Pool(4) as pool:
            for seed, src, res, extra in pool.imap_unordered(one, jobs):
                if res != "ok":
                    bad.append((seed, os.path.basename(src), res, extra))
                elif extra == "E":
                    errs += 1
        for b in sorted(bad)[:30]:
            print("BAD", b)
        print(f"{ROUNDS} damaged files: {len(bad)} crashes/panics/hangs, {errs} answered an error, {ROUNDS - len(bad) - errs} had no error at all")
        sys.exit(1 if bad else 0)
