#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/check_bt.py <bt_probe> -- the B-tree of lib/db against SQLite.
# SQLite makes the file, bt_probe inserts/deletes rows in the table's root
# page, SQLite then runs PRAGMA integrity_check and reads every row back.
# Twice: with the rowids in pseudo-random order (splits in the middle of the tree) and in
# sequence (appends to the right-most leaf: the quick path that gives the new cell a page of its own).
import os, sqlite3, subprocess, sys, tempfile

probe = sys.argv[1]
fails = 0
SEQ = False

def rowid_of(i):
    if SEQ:
        return i + 1
    return (i * 2654435761) % 4294967296 + 1

def text_of(i, maxlen):
    n = (i * 31 + 7) % maxlen
    return ''.join(chr(97 + (i + j) % 26) for j in range(n))

def run(cmd, *args):
    name = cmd + "s" if SEQ and cmd in ("ins", "del") else cmd
    return subprocess.run([probe, name] + [str(a) for a in args], capture_output=True, text=True)

def make(path, page_size):
    c = sqlite3.connect(path)
    c.execute(f"PRAGMA page_size={page_size}")
    c.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, b TEXT)")
    c.commit()
    root = c.execute("SELECT rootpage FROM sqlite_master WHERE name='t'").fetchone()[0]
    c.close()
    return root

def verify(path, expect, label):
    global fails
    c = sqlite3.connect(path)
    ic = c.execute("PRAGMA integrity_check").fetchall()
    rows = c.execute("SELECT id, b FROM t ORDER BY id").fetchall()
    c.close()
    ok = ic == [('ok',)] and rows == sorted(expect.items())
    if not ok:
        fails += 1
        print(f"FAIL {label}: integrity={ic[:3]} rows={len(rows)} expected={len(expect)}")
    else:
        print(f"ok   {label}: {len(rows)} rows, integrity ok")

with tempfile.TemporaryDirectory() as d:
    for SEQ in (False, True):
        for page_size in (512, 1024, 4096, 65536):
            for maxlen in (20, 300, 9000):
                tag = "seq" if SEQ else "rnd"
                path = os.path.join(d, f"t{tag}{page_size}_{maxlen}.db")
                root = make(path, page_size)
                expect = {}
                n1 = 1500 if maxlen < 1000 else 300
                label = f"{tag} ps={page_size} maxlen={maxlen}"
                r = run("ins", path, root, n1, maxlen, 0)
                if r.returncode: print("FAIL ins", r.stderr); fails += 1; continue
                for i in range(0, n1): expect[rowid_of(i)] = text_of(i, maxlen)
                verify(path, expect, f"{label} insert {n1}")
                r = run("ins", path, root, n1, maxlen, n1)
                if r.returncode: print("FAIL ins2", r.stderr); fails += 1; continue
                for i in range(n1, 2 * n1): expect[rowid_of(i)] = text_of(i, maxlen)
                verify(path, expect, f"{label} insert {2*n1}")
                r = run("del", path, root, n1, maxlen, 0)
                if r.returncode: print("FAIL del", r.stderr); fails += 1; continue
                for i in range(0, n1): del expect[rowid_of(i)]
                verify(path, expect, f"{label} delete {n1}")
                r = run("del", path, root, n1, maxlen, n1)
                if r.returncode: print("FAIL del2", r.stderr); fails += 1; continue
                for i in range(n1, 2 * n1): del expect[rowid_of(i)]
                verify(path, expect, f"{label} delete all")
                r = run("check", path, root, 0)
                print("   ", r.stdout.strip(), r.stderr.strip())
print("FAILED" if fails else "ALL OK")
sys.exit(1 if fails else 0)
