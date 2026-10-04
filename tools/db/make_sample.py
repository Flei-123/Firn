#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/make_sample.py <out.db> -- the database tests/2164_db_readsqlite.fi reads: made by
# SQLite (Python's sqlite3), small pages so the trees are several levels deep, an overflow
# value, NULLs, REAL, BLOB, unicode, an index, a dropped table (a freelist).
import os, sqlite3, sys

out = sys.argv[1]
if os.path.exists(out):
    os.unlink(out)
c = sqlite3.connect(out)
c.execute("PRAGMA page_size=512")
c.execute("PRAGMA user_version=42")
c.executescript("""
CREATE TABLE countries(code TEXT PRIMARY KEY, name TEXT NOT NULL, pop INTEGER, area REAL);
CREATE TABLE cities(id INTEGER PRIMARY KEY, country TEXT, name TEXT, pop INTEGER, flag BLOB);
CREATE INDEX cities_country ON cities(country);
CREATE INDEX cities_pop ON cities(pop DESC);
CREATE TABLE scratch(x);
""")
countries = [("AT", "Austria", 9100000, 83879.0), ("DE", "Germany", 84000000, 357022.0), ("CH", "Switzerland", 8800000, 41285.5),
             ("FR", "France", 68000000, None), ("IT", "Italy", None, 301340.0), ("JP", "日本", 125000000, 377975.0)]
c.executemany("INSERT INTO countries VALUES (?,?,?,?)", countries)
names = ["Alpha", "Beta", "Gamma", "Delta", "Épsilon", "Zeta", "Eta", "Theta", "Iota", "Kappa"]
rows = []
for i in range(1, 241):
    code = countries[i % 6][0]
    flag = bytes([i % 256, (i * 7) % 256, 0, 255]) if i % 5 == 0 else None
    rows.append((i, code, f"{names[i % 10]}{i}", (i * 7919) % 100000 if i % 17 else None, flag))
c.executemany("INSERT INTO cities VALUES (?,?,?,?,?)", rows)
c.execute("INSERT INTO cities VALUES (1000, 'AT', ?, 1, NULL)", ("long text " * 600,))
c.executemany("INSERT INTO scratch VALUES (?)", [(i,) for i in range(300)])
c.commit()
c.execute("DROP TABLE scratch")
c.commit()
c.close()
print("wrote", out, os.path.getsize(out), "octets")
