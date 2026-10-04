#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/check_parse.py <parse_probe> -- the parser of lib/db against SQLite's.
# Every line of parse_corpus.txt starts with '+' (SQLite accepts it, so must we)
# or '-' (SQLite reports a syntax error, so must we). Errors of SQLite that are
# not syntax errors (no such table ...) count as "parsed".
import subprocess, sqlite3, sys, os

probe = sys.argv[1]
here = os.path.dirname(os.path.abspath(__file__))
conn = sqlite3.connect(":memory:")
for ddl in ("CREATE TABLE t(a,b,c,d,e,x,y,z,id,v)", "CREATE TABLE u(a,b,z)", "CREATE TABLE w(a,z)"):
    conn.execute(ddl)

def sqlite_ok(sql):
    for part in sql.split(";"):
        part = part.strip()
        if not part:
            continue
        try:
            conn.execute("EXPLAIN " + part)
        except sqlite3.Error as e:
            m = str(e)
            if "syntax error" in m or "incomplete input" in m or "unrecognized token" in m:
                return False
            if "cannot start a transaction" in m or "no transaction" in m:
                continue
    return True

fails = 0
n = 0
for line in open(os.path.join(here, "parse_corpus.txt"), encoding="utf-8"):
    line = line.rstrip("\n")
    if not line:
        continue
    want = line[0] == "+"
    sql = line[1:]
    n += 1
    theirs = sqlite_ok(sql)
    r = subprocess.run([probe, sql], capture_output=True, text=True)
    mine = r.returncode == 0 and r.stdout.startswith("OK")
    if theirs != want:
        print(f"CORPUS WRONG (sqlite says {theirs}): {sql}")
        fails += 1
    elif mine != want:
        print(f"FAIL (mine={mine}, want={want}): {sql}    {r.stderr.strip()}")
        fails += 1
print(f"{n} statements, {fails} differences")
sys.exit(1 if fails else 0)
