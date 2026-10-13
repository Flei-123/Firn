#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dns/check6.py <dns_main binary> -- the resolver with an IPv6 NAME
SERVER, against tools/dns/fake_dns.py listening on [::1] (UDP and TCP).
Loopback only: no real IPv6 route is needed or used."""
import os, subprocess, sys, tempfile

BIN = sys.argv[1]
HERE = os.path.dirname(os.path.abspath(__file__))
fails = 0


def check(ok, what):
    global fails
    print(('  ok   ' if ok else '  FAIL ') + what)
    if not ok:
        fails += 1


def start():
    p = subprocess.Popen([sys.executable, os.path.join(HERE, 'fake_dns.py'), '6'],
                         stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    port = int(p.stdout.readline().split()[1])
    return p, port


def ask(port, *args, extra=()):
    r = subprocess.run([BIN, '-s', '[::1]:%d' % port, '-w', '500', '-r', '2', *extra, *args],
                       capture_output=True, text=True, timeout=30)
    return r.stdout.strip().splitlines()


srv, port = start()
try:
    out = ask(port, 'plain.test')
    check(out == ['plain.test OK udp 60 0 192.0.2.1'], 'A over UDP to [::1]: %r' % out)
    out = ask(port, 'dual.test', extra=['-6'])
    check(out and out[0].startswith('dual.test OK udp 60 0 ') and out[0].endswith('2001:0db8:0000:0000:0000:0000:0000:0007')
          or (out and '2001' in out[0] and 'OK udp' in out[0]), 'AAAA over UDP to [::1]: %r' % out)
    out = ask(port, 'chain.test')
    check(out and out[0].startswith('chain.test OK') and '192.0.2.2' in out[0], 'CNAME chain: %r' % out)
    out = ask(port, 'big.test')
    check(out and ' tcp ' in out[0] and out[0].count('192.0.2.') >= 8, 'TC -> TCP fallback over IPv6: %r' % out)
    out = ask(port, 'nx.test')
    check(out and ' ERR NxDomain' in out[0], 'NXDOMAIN: %r' % out)
    out = ask(port, 'fail.test')
    check(out and ' ERR ServerFail' in out[0], 'SERVFAIL: %r' % out)
    out = ask(port, 'silent.test')
    check(out and ' ERR Timeout' in out[0], 'silent server -> Timeout: %r' % out)
    # plain address without brackets and port 53 is NOT reachable here: Network/Timeout, never a crash
    r = subprocess.run([BIN, '-s', '::1', '-w', '200', '-r', '1', 'plain.test'],
                       capture_output=True, text=True, timeout=30)
    check(' ERR ' in r.stdout and r.returncode == 0, 'bare ::1 (port 53, nobody there) fails cleanly: %r' % r.stdout.strip())
finally:
    srv.kill()
    err = srv.stderr.read()
    qs = [l for l in err.splitlines() if l.startswith('Q ')]
    check(any('big.test' in l and l.endswith('tcp') for l in qs), 'the server saw the TCP question')
print('RESULT dns6: %s' % ('ok' if fails == 0 else '%d FAILED' % fails))
sys.exit(1 if fails else 0)
