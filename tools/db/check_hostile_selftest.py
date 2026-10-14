#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/db/check_hostile_selftest.py -- r581 regression test for check_hostile.one().
#
# Under Wine the first client forks a wineserver daemon that inherits stdout/stderr and lives
# on while other jobs keep it busy. A runner that reads the child's output through PIPES then
# blocks until the daemon exits and reports a false TIMEOUT (seen as "Seed 1003 hangs under
# Wine"). Here a fake runner leaves such a daemon behind: one() must still answer "ok" at once.
# The negative control shows the pipe-based call really blocks on the same fake runner.
import os, shutil, sqlite3, subprocess, sys, tempfile, time

here = os.path.dirname(os.path.abspath(__file__))
d = tempfile.mkdtemp(prefix="hostile-selftest-")
fails = []
try:
    fake = os.path.join(d, "runner.sh")
    pidf = os.path.join(d, "daemon.pid")
    open(fake, "w").write('#!/bin/sh\nsetsid sleep 60 &\necho $! > "%s"\necho OK\nexit 0\n' % pidf)
    os.chmod(fake, 0o755)
    src = os.path.join(d, "src.db")
    c = sqlite3.connect(src)
    c.execute("CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER)")
    c.executemany("INSERT INTO t(a) VALUES (?)", [(i,) for i in range(50)])
    c.commit()
    c.close()
    q = os.path.join(d, "q.sql")
    open(q, "w").write("SELECT 1\n")

    # negative control: the old way (pipes) blocks while the daemon holds them
    t = time.time()
    try:
        subprocess.run([fake, "x", "y"], capture_output=True, timeout=3)
        blocked = False
    except subprocess.TimeoutExpired:
        blocked = True
    print("  %s negative control: pipe read blocks on the daemon (%.1f s)" % ("OK  " if blocked else "FAIL", time.time() - t))
    if not blocked:
        fails.append("negative control")

    os.environ["RUNNER"] = fake
    sys.argv = ["check_hostile.py", "unused-probe", "1"]
    sys.path.insert(0, here)
    import check_hostile
    t = time.time()
    seed, _, res, _ = check_hostile.one((src, 1003, q))
    dt = time.time() - t
    good = res == "ok" and dt < 10
    print("  %s one() with a daemon left behind: %s in %.1f s" % ("OK  " if good else "FAIL", res, dt))
    if not good:
        fails.append("one()")
finally:
    try:
        for p in open(pidf).read().split():
            os.kill(int(p), 15)
    except Exception:
        pass
    shutil.rmtree(d, ignore_errors=True)
print("hostile selftest: %d failed" % len(fails) if fails else "hostile selftest: all checks passed")
sys.exit(1 if fails else 0)
