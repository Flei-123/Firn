#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/check_idx.py <bt_probe> -- the INDEX B-tree of lib/db against SQLite.
import os, sqlite3, subprocess, sys, tempfile
probe = sys.argv[1]
fails = 0
def rowid_of(i): return (i * 2654435761) % 4294967296 + 1
def text_of(i, maxlen):
    n = (i * 31 + 7) % maxlen
    return ''.join(chr(97 + (i + j) % 26) for j in range(n))
def run(*a):
    return subprocess.run([probe] + [str(x) for x in a], capture_output=True, text=True)
def make(path, ps):
    c = sqlite3.connect(path)
    c.execute(f"PRAGMA page_size={ps}")
    c.execute("CREATE TABLE u(id INTEGER PRIMARY KEY, b TEXT)")
    c.execute("CREATE INDEX ub ON u(b)")
    c.commit()
    r = {n: p for n, p in c.execute("SELECT name, rootpage FROM sqlite_master")}
    c.close()
    return r['u'], r['ub']
def verify(path, expect, label):
    global fails
    c = sqlite3.connect(path)
    ic = c.execute("PRAGMA integrity_check").fetchall()
    rows = c.execute("SELECT id, b FROM u ORDER BY id").fetchall()
    via = c.execute("SELECT id, b FROM u INDEXED BY ub ORDER BY b, id").fetchall()
    c.close()
    ok = ic == [('ok',)] and rows == sorted(expect.items()) and via == sorted(expect.items(), key=lambda kv: (kv[1].encode(), kv[0]))
    if not ok:
        fails += 1
        print(f"FAIL {label}: integrity={ic[:3]} rows={len(rows)} via_index={len(via)} expected={len(expect)}")
    else:
        print(f"ok   {label}: {len(rows)} rows, integrity ok")
with tempfile.TemporaryDirectory() as d:
    for ps in (512, 1024, 4096):
        for maxlen in (10, 120, 2500):
            path = os.path.join(d, f"i{ps}_{maxlen}.db")
            root, ir = make(path, ps)
            n1 = 1200 if maxlen < 1000 else 250
            expect = {}
            for batch, (cmd, a, b) in enumerate((("insi", 0, n1), ("insi", n1, 2 * n1), ("deli", 0, n1 // 2), ("deli", n1 // 2, n1 // 2 + n1 // 4), ("insi", 0, n1 // 3), ("deli", n1, 2 * n1))):
                if cmd == "insi":
                    r = run("insi", path, root, ir, b - a, maxlen, a)
                    for i in range(a, b): expect[rowid_of(i)] = text_of(i, maxlen)
                else:
                    r = run("deli", path, root, ir, b - a, maxlen, a)
                    for i in range(a, b): expect.pop(rowid_of(i), None)
                if r.returncode or r.stderr.strip():
                    print("FAIL run", cmd, a, b, r.stderr.strip()[:200]); fails += 1; break
                verify(path, expect, f"ps={ps} maxlen={maxlen} {cmd} {a}..{b}")
            r = run("check", path, ir, 1)
            print("   ", r.stdout.strip(), r.stderr.strip())
print("FAILED" if fails else "ALL OK")
sys.exit(1 if fails else 0)
