#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dns/mini_check.py -- lib/net/dnsmini.fi (the #[no_gc] A lookup of
the Android push thread) against tools/dns/fake_dns.py.

    mini_check.py <dnsmini_main binary>

What the SERVER saw is counted from its stderr log (`Q <name> <type> <udp|tcp>`).
"""
import os
import subprocess
import sys
import time

here = os.path.dirname(os.path.abspath(__file__))
tool = sys.argv[1]
passed = failed = 0


def check(label, got, want):
    global passed, failed
    if got == want:
        passed += 1
        print("  ok    %s" % label)
    else:
        failed += 1
        print("  FAIL  %s: got %r, want %r" % (label, got, want))


srv = subprocess.Popen([sys.executable, os.path.join(here, "fake_dns.py")],
                       stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
port = int(srv.stdout.readline().split()[1])


def run(*names, timeout_ms=700, port_=None):
    p = subprocess.run([tool, "127.0.0.1", str(port_ or port), str(timeout_ms), *names],
                       capture_output=True, text=True, timeout=60)
    return dict(l.split(" ", 1) for l in p.stdout.splitlines() if l)


try:
    r = run("plain.test", "chain.test", "dual.test", "nx.test", "fail.test",
            "big.test", "PLAIN.test", "plain.test.", "bad..name", "-x" * 40 + ".test")
    check("plain A", r.get("plain.test"), "OK 192.0.2.1")
    check("CNAME chain (compressed names) -> the A record", r.get("chain.test"), "OK 192.0.2.2")
    check("dual-stack name gives its A record", r.get("dual.test"), "OK 192.0.2.7")
    check("NXDOMAIN is no address", r.get("nx.test"), "ERR")
    check("SERVFAIL is no address", r.get("fail.test"), "ERR")
    check("truncated reply without records is a failure", r.get("big.test"), "ERR")
    check("name case does not matter", r.get("PLAIN.test"), "OK 192.0.2.1")
    check("trailing dot", r.get("plain.test."), "OK 192.0.2.1")
    check("empty label is refused locally", r.get("bad..name"), "ERR")
    long_label = "a" * 64 + ".test"
    check("label of 64 octets refused locally", run(long_label).get(long_label), "ERR")
    t0 = time.time()
    r = run("silent.test", timeout_ms=300)
    dt = time.time() - t0
    check("silent server: ERR", r.get("silent.test"), "ERR")
    check("silent server: 2 rounds x 300 ms, not forever", 0.5 <= dt < 3.0, True)
    # nothing listening at all
    r = run("plain.test", timeout_ms=300, port_=1)
    check("dead port: ERR", r.get("plain.test"), "ERR")
finally:
    srv.terminate()
    log = srv.stderr.read()
    srv.wait()

qs = [l.split() for l in log.splitlines() if l.startswith("Q ")]
names = [q[1] for q in qs]
check("the server saw the questions", names.count("plain.test") >= 2, True)
check("an invalid name never left the machine",
      any(n.startswith("bad") or n.startswith("-x") or n.startswith("a" * 64) for n in names), False)
check("only type A (1) was asked", sorted({q[2] for q in qs} - {"1", "A"}), [])
print("mini_check: %d checks, %d failed" % (passed + failed, failed))
sys.exit(1 if failed else 0)
