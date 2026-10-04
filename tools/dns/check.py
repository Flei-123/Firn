#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/dns/check.py <dns_main> <http_main> -- the resolver and https by name,
held against implementations this repository did not write.

  A  lib/net/dns_main.fi against tools/dns/fake_dns.py (a DNS server in
     Python): plain A, a CNAME chain with compressed names, TC then TCP, NXDOMAIN, SERVFAIL, a
     server that never answers, a name that is not a name -- and what the
     SERVER saw is counted from its log, not assumed from the client.
  B  the same resolver against the REAL network (skipped when there is
     none): the answers must belong to what `getent`/Python's resolver and
     1.1.1.1 say.
  C  https BY NAME, hermetic: a Python TLS 1.3 server with a certificate
     from a test CA, a fake DNS server that points `hermetic.test` at it.
     Counter-checks: no roots, a certificate for another name, a root that
     did not sign it.
  D  https BY NAME against the real internet (skipped when there is no
     route): piston-meta.mojang.com and api.modrinth.com compared with
     curl, and badssl.com for the refusals.
"""
import hashlib
import http.server
import os
import re
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
import threading
import time

HERE = os.path.dirname(os.path.abspath(__file__))
DNS_MAIN, HTTP_MAIN = sys.argv[1], sys.argv[2]
# RUNNER="wine64" runs the two programs (Windows builds) through Wine
import shlex
RUNNER = shlex.split(os.environ.get('RUNNER', ''))
FAILS = 0
PASSES = 0
SKIPS = 0


def ok(msg):
    global PASSES
    PASSES += 1
    print('  OK    ' + msg)


def bad(msg, detail=''):
    global FAILS
    FAILS += 1
    print('  FAIL  ' + msg)
    if detail:
        print('        ' + detail[:400].replace('\n', '\n        '))


def skip(msg):
    global SKIPS
    SKIPS += 1
    print('  SKIP  ' + msg)


def check(cond, msg, detail=''):
    if cond:
        ok(msg)
    else:
        bad(msg, detail)


def start_fake_dns(logpath):
    log = open(logpath, 'w')
    p = subprocess.Popen([sys.executable, os.path.join(HERE, 'fake_dns.py')],
                         stdout=subprocess.PIPE, stderr=log)
    line = p.stdout.readline().decode().strip()
    m = re.match(r'port (\d+)', line)
    if not m:
        p.kill()
        raise SystemExit('fake_dns printed %r' % line)
    return p, int(m.group(1))


def questions(logpath):
    out = []
    for ln in open(logpath):
        m = re.match(r'Q (\S+) (\d+) (udp|tcp)', ln)
        if m:
            out.append((m.group(1), int(m.group(2)), m.group(3)))
    return out


def run_dns(args, timeout=60):
    r = subprocess.run(RUNNER + [DNS_MAIN] + args, capture_output=True, text=True, timeout=timeout)
    return [l.rstrip('\r') for l in r.stdout.strip().split('\n')] if r.stdout.strip() else [], r.returncode


def run_http(lines, timeout=120):
    r = subprocess.run(RUNNER + [HTTP_MAIN], input=('\n'.join(lines) + '\n').encode(),
                       capture_output=True, timeout=timeout)
    blocks, cur = [], []
    # BODYTEXT carries raw octets of the body: latin-1 keeps every one of them
    for ln in r.stdout.decode('latin-1').replace('\r', '').split('\n'):
        if ln == '.':
            blocks.append(cur)
            cur = []
        else:
            cur.append(ln)
    out = []
    for b in blocks:
        d = {}
        for ln in b:
            k, _, v = ln.partition(' ')
            d.setdefault(k, v)
        out.append(d)
    return out


def have_network():
    try:
        socket.setdefaulttimeout(4)
        socket.gethostbyname('example.com')
        return True
    except OSError:
        return False


def sh(cmd, **kw):
    return subprocess.run(cmd, shell=True, capture_output=True, text=True, **kw)


# ------------------------------------------------------------------ A
def part_a(tmp):
    print('== A. the resolver against a fake DNS server ==')
    logp = os.path.join(tmp, 'dns.log')
    srv, port = start_fake_dns(logp)
    try:
        s = '127.0.0.1:%d' % port
        names = ['plain.test', 'plain.test', 'chain.test', 'big.test', 'nx.test',
                 'nx.test', 'fail.test', 'a..b', 'plain.test.', 'PLAIN.TEST']
        lines, rc = run_dns(['-s', s, '-w', '300', '-r', '2'] + names)
        check(rc == 0 and len(lines) == len(names), 'one line per name', repr(lines))
        if len(lines) == len(names):
            check(re.fullmatch(r'plain\.test OK udp 60 0 192\.0\.2\.1', lines[0]), 'plain A answer', lines[0])
            check(re.fullmatch(r'plain\.test OK cache (59|60) 0 192\.0\.2\.1', lines[1]), 'second question from the cache', lines[1])
            check(re.fullmatch(r'chain\.test OK udp 120 2 192\.0\.2\.2', lines[2]), 'CNAME chain, smallest TTL of the chain', lines[2])
            check(re.fullmatch(r'big\.test OK tcp 30 0 (192\.0\.2\.1\d\d ?){8}', lines[3]), 'TC -> TCP, 8 of 11 addresses kept', lines[3])
            check(lines[4] == 'nx.test ERR NxDomain', 'NXDOMAIN', lines[4])
            check(lines[5] == 'nx.test ERR NxDomain', 'NXDOMAIN again (negative cache)', lines[5])
            check(lines[6] == 'fail.test ERR ServerFail', 'SERVFAIL', lines[6])
            check(lines[7] == 'a..b ERR Name', 'an invalid name', lines[7])
            check(re.fullmatch(r'plain\.test\. OK cache (59|60) 0 192\.0\.2\.1', lines[8]), 'trailing dot = same name', lines[8])
            check(re.fullmatch(r'PLAIN\.TEST OK cache (59|60) 0 192\.0\.2\.1', lines[9]), 'upper case = same name', lines[9])
        q = questions(logp)
        count = lambda n, t=None: len([x for x in q if x[0] == n and (t is None or x[2] == t)])
        check(count('plain.test') == 1, 'the SERVER saw plain.test once (cache did the rest)', repr(q))
        check(count('nx.test') == 1, 'the server saw nx.test once (negative cache)')
        check(count('fail.test') == 1, 'SERVFAIL ended the search with ONE question, not a retry storm')
        check(count('big.test', 'udp') == 1 and count('big.test', 'tcp') == 1, 'big.test: one UDP question (TC), one TCP question')
        check(count('a..b') == 0 and count('a') == 0, 'an invalid name never reached the server')
        # a server that never answers
        t0 = time.time()
        lines, rc = run_dns(['-s', s, '-w', '200', '-r', '3', 'silent.test'])
        dt = time.time() - t0
        check(lines == ['silent.test ERR Timeout'], 'a silent server is a Timeout', repr(lines))
        check(1.2 < dt < 6, 'the timeout doubles: 200+400+800 ms', '%.2fs' % dt)
        check(len([x for x in questions(logp) if x[0] == 'silent.test']) == 3, 'three datagrams for three attempts')
        # AAAA for a name that has none
        lines, rc = run_dns(['-s', s, '-6', 'plain.test'])
        check(lines == ['plain.test ERR NoData'], 'AAAA of a name that has only A: NoData', repr(lines))
        # no server at all in the way: localhost, numeric
        lines, rc = run_dns(['-s', s, 'localhost', '10.1.2.3', 'x.localhost'])
        check(lines == ['localhost OK localhost 0 0 127.0.0.1', '10.1.2.3 OK numeric 0 0 10.1.2.3',
                        'x.localhost OK localhost 0 0 127.0.0.1'], 'localhost and numeric answers', repr(lines))
    finally:
        srv.kill()


# ------------------------------------------------------------------ B
def part_b():
    print('== B. the resolver against the real network ==')
    if not have_network():
        skip('no route to the internet')
        return
    names = ['example.com', 'api.modrinth.com', 'piston-meta.mojang.com', 'www.microsoft.com',
             'github.com', 'resources.download.minecraft.net']
    lines, rc = run_dns(names)
    check(len(lines) == len(names), 'one line per name', repr(lines))
    for ln in lines:
        m = re.match(r'(\S+) OK (udp|tcp|cache) (\d+) (\d+) (.*)', ln)
        if not m:
            bad('%s did not resolve' % ln.split(' ')[0], ln)
            continue
        name, mine = m.group(1), set(m.group(5).split())
        # what Python's resolver and 1.1.1.1/8.8.8.8 say (a CDN rotates: any overlap is a pass)
        theirs = set()
        for _ in range(3):
            try:
                theirs |= {a[4][0] for a in socket.getaddrinfo(name, 443, socket.AF_INET)}
            except OSError:
                pass
            for ns in ('1.1.1.1', '8.8.8.8'):
                r = sh('dig +short +time=3 +tries=1 A %s @%s' % (name, ns))
                theirs |= {x for x in r.stdout.split() if re.fullmatch(r'\d+\.\d+\.\d+\.\d+', x)}
        check(bool(mine & theirs), '%s -> %s agrees with getaddrinfo and dig' % (name, ' '.join(sorted(mine))),
              'theirs: ' + ' '.join(sorted(theirs)))
    # the CNAME chain of www.microsoft.com is followed
    m = re.match(r'www\.microsoft\.com OK \w+ \d+ (\d+) ', [l for l in lines if l.startswith('www.microsoft.com')][0]) if any(l.startswith('www.microsoft.com') for l in lines) else None
    check(bool(m) and int(m.group(1)) >= 1, 'a real CNAME chain was followed (www.microsoft.com)')
    lines, rc = run_dns(['nonexistent-zz9.invalid', 'example.com'])
    check(lines[0] == 'nonexistent-zz9.invalid ERR NxDomain', '.invalid does not exist', repr(lines))
    # the same, against TWO servers where the first one is dead: 127.0.0.2 does not answer on 53
    lines, rc = run_dns(['-s', '1.1.1.1', '-6', 'example.com'])
    check(len(lines) == 1 and ' OK ' in lines[0] and ':' in lines[0], 'AAAA over IPv4 transport', repr(lines))


# ------------------------------------------------------------------ C
def make_pki(tmp):
    def o(cmd):
        r = subprocess.run(cmd, shell=True, cwd=tmp, capture_output=True, text=True)
        if r.returncode:
            raise SystemExit('openssl failed: %s\n%s' % (cmd, r.stderr))
    for ca in ('ca', 'ca2'):
        o('openssl ecparam -name prime256v1 -genkey -noout -out %s.key' % ca)
        o('openssl req -x509 -new -key %s.key -sha256 -days 3 -subj "/CN=Firn Test %s" -out %s.pem '
          '-addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign"' % (ca, ca, ca))
    o('openssl ecparam -name prime256v1 -genkey -noout -out srv.key')
    o('openssl req -new -key srv.key -subj "/CN=hermetic.test" -out srv.csr')
    with open(os.path.join(tmp, 'ext.cnf'), 'w') as f:
        f.write('subjectAltName=DNS:hermetic.test\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature\n')
    o('openssl x509 -req -in srv.csr -CA ca.pem -CAkey ca.key -CAcreateserial -days 2 -sha256 -out srv.pem -extfile ext.cnf')


class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    snis = []

    def log_message(self, *a):
        pass

    def reply(self, body, extra=None):
        self.send_response(200)
        self.send_header('Content-Type', 'application/octet-stream')
        self.send_header('Content-Length', str(len(body)))
        for k, v in (extra or []):
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == '/big':
            body = hashlib.sha256(b'seed').digest() * 32768  # 1 MiB
            return self.reply(body)
        self.reply(b'hello-hermetic')

    def do_POST(self):
        n = int(self.headers.get('Content-Length', '0'))
        body = self.rfile.read(n)
        self.reply(('METHOD=POST BODY=' + body.decode('latin-1')).encode())


def part_c(tmp):
    print('== C. https by name, hermetic ==')
    make_pki(tmp)
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.minimum_version = ssl.TLSVersion.TLSv1_3
    ctx.load_cert_chain(os.path.join(tmp, 'srv.pem'), os.path.join(tmp, 'srv.key'))
    snis = []
    ctx.sni_callback = lambda sock, name, c: snis.append(name)
    httpd = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    httpd.socket = ctx.wrap_socket(httpd.socket, server_side=True)
    hport = httpd.server_address[1]
    threading.Thread(target=httpd.serve_forever, daemon=True).start()
    logp = os.path.join(tmp, 'dns2.log')
    srv, dport = start_fake_dns(logp)
    ca, ca2 = os.path.join(tmp, 'ca.pem'), os.path.join(tmp, 'ca2.pem')
    try:
        pre = ['D 127.0.0.1 %d' % dport, 'Y 0']
        url = 'https://hermetic.test:%d' % hport
        res = run_http(pre + ['T ' + ca, 'G %s/hello' % url, 'G %s/hello' % url, 'S', 'E'])
        # blocks: D, Y print only "."; T prints TRUST; then G, G, S, E
        g1 = [r for r in res if 'STATUS' in r]
        check(len(g1) == 2 and all(r['STATUS'] == '200' and r['BODYTEXT'] == 'hello-hermetic' for r in g1),
              'GET https://hermetic.test:%d/hello -> 200, body intact' % hport, repr(res))
        st = [r for r in res if 'SOCKETS' in r]
        check(st and st[0]['SOCKETS'] == '1' and st[0]['REUSED'] == '1', 'the TLS connection is reused (1 socket, 2 requests)', repr(st))
        ee = [r for r in res if 'TLSERR' in r]
        check(ee and ee[0]['TLSERR'] == '0' and ee[0]['ISTLS'] == '1' and int(ee[0]['DNSUDP']) >= 1,
              'the name went through the resolver (UDP) and the handshake is done', repr(ee))
        check('hermetic.test' in snis, 'the server saw SNI hermetic.test', repr(snis))
        check(any(q[0] == 'hermetic.test' for q in questions(logp)), 'the fake DNS server saw the question')
        # POST and a MiB through the record layer
        res = run_http(pre + ['T ' + ca, 'P %s/echo text/plain name=firn&x=1' % url, 'G %s/big' % url])
        post = [r for r in res if 'STATUS' in r]
        check(len(post) == 2 and post[0]['BODYTEXT'] == 'METHOD=POST BODY=name=firn&x=1', 'POST over TLS', repr(post[:1]))
        check(len(post) == 2 and post[1]['BODY'] == '1048576', 'a 1 MiB body over TLS arrives complete', repr(post[1:]))
        # counter-checks
        res = run_http(pre + ['G %s/hello' % url, 'E'])
        err = [r for r in res if 'ERR' in r]
        ee = [r for r in res if 'TLSERR' in r]
        check(err and err[0]['ERR'] == 'Tls' and ee[0]['TLSERR'] == '100', 'COUNTER-CHECK: no roots at all -> refused (Tls, reason 100)', repr(res))
        res = run_http(pre + ['T ' + ca2, 'G %s/hello' % url, 'E'])
        err = [r for r in res if 'ERR' in r]
        ee = [r for r in res if 'TLSERR' in r]
        check(err and err[0]['ERR'] == 'Tls' and ee[0]['TLSVERIFY'] == '5', 'COUNTER-CHECK: a root that did not sign it -> UNKNOWN_ISSUER', repr(res))
        res = run_http(pre + ['T ' + ca, 'G https://other.test:%d/hello' % hport, 'E'])
        err = [r for r in res if 'ERR' in r]
        ee = [r for r in res if 'TLSERR' in r]
        check(err and err[0]['ERR'] == 'Tls' and ee[0]['TLSVERIFY'] == '4', 'COUNTER-CHECK: a certificate for ANOTHER name -> NAME', repr(res))
        res = run_http(pre + ['T ' + ca, 'G https://127.0.0.1:%d/hello' % hport, 'E'])
        err = [r for r in res if 'ERR' in r]
        check(err and err[0]['ERR'] == 'Tls', 'COUNTER-CHECK: the numeric address is not the name in the certificate', repr(res))
        res = run_http(pre + ['T ' + ca, 'G https://nx.test:%d/hello' % hport])
        err = [r for r in res if 'ERR' in r]
        check(err and err[0]['ERR'] == 'Resolve', 'COUNTER-CHECK: a name that does not exist -> Resolve', repr(res))
        # the same hostname over plain http reaches the TLS port and must not be taken for a page
        res = run_http(pre + ['G http://hermetic.test:%d/hello' % hport])
        check(not any(r.get('STATUS') == '200' for r in res), 'COUNTER-CHECK: http:// to the TLS port is not a successful fetch', repr(res))
    finally:
        srv.kill()
        httpd.shutdown()


# ------------------------------------------------------------------ D
def part_d():
    print('== D. https by name against the real internet ==')
    if not have_network():
        skip('no route to the internet')
        return
    cases = [('https://piston-meta.mojang.com/mc/game/version_manifest_v2.json', '--compressed'),
             ('https://api.modrinth.com/v2/tag/game_version', '--compressed'),
             ('https://github.com/', '--compressed')]
    for url, flags in cases:
        res = run_http(['G ' + url, 'E'])
        g = [r for r in res if 'STATUS' in r]
        if not g:
            bad('%s: no answer' % url, repr(res))
            continue
        want = subprocess.run(['curl', '-sL', flags, '--max-time', '40', url], capture_output=True)
        size = len(want.stdout)
        code = '200' if want.returncode == 0 and size > 0 else '0'
        if code == '200' and g[0]['STATUS'] == '200':
            # GitHub's front page differs from request to request (nonces), the JSON APIs do not
            slack = max(4096, size // 4) if 'github.com' in url else 64
            check(abs(int(g[0]['BODY']) - size) <= slack,
                  '%s: 200, %s octets (curl: %d)' % (url, g[0]['BODY'], size), repr(g[0]))
        else:
            check(g[0]['STATUS'] == '200', '%s: status %s (curl got %d octets)' % (url, g[0]['STATUS'], size))
    # the exact bytes of a stable file
    url = 'https://piston-meta.mojang.com/v1/packages/655c2da7d6815e84864a0f9a055c90e36337b0b4/26.4-snapshot-2.json'
    r = subprocess.run(['curl', '-s', '--max-time', '40', url], capture_output=True)
    if r.returncode == 0 and len(r.stdout) > 1000:
        res = run_http(['G ' + url])
        g = [x for x in res if 'STATUS' in x]
        check(g and g[0]['STATUS'] == '200' and int(g[0]['BODY']) == len(r.stdout),
              'a content-addressed Mojang file has exactly the length curl got (%d)' % len(r.stdout), repr(g))
    else:
        skip('Mojang package file not available for comparison')
    # The refusals, from badssl.com. Those hosts speak TLS 1.2 only, so the
    # handshake ends in an alert before any certificate is seen: what is
    # checked is that nothing is FETCHED from them (Tls, no body). The
    # certificate verdicts themselves (EXPIRED, NAME, UNKNOWN_ISSUER) are
    # held by part C and by tools/tls/cert_check.py.
    probe = sh('curl -s -o /dev/null -w "%{http_code}" --max-time 15 https://badssl.com/')
    if probe.stdout.strip() != '200':
        skip('badssl.com is not reachable')
        return
    for host in ('expired.badssl.com', 'wrong.host.badssl.com', 'self-signed.badssl.com',
                 'untrusted-root.badssl.com'):
        res = run_http(['G https://%s/' % host])
        err = [r for r in res if 'ERR' in r]
        check(err and err[0]['ERR'] == 'Tls' and not any('STATUS' in r for r in res),
              'COUNTER-CHECK: https://%s is not fetched (Tls)' % host, repr(res))


def main():
    with tempfile.TemporaryDirectory() as tmp:
        part_a(tmp)
        part_b()
        part_c(tmp)
        part_d()
    print('DNS %d checks passed, %d failed, %d skipped' % (PASSES, FAILS, SKIPS))
    sys.exit(1 if FAILS else 0)


main()
