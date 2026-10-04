#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/download/check.py <dl_main> -- lib/net/download.fi held against a
server that misbehaves on purpose (tools/download/fake_server.py) and, when
there is a route, against the real Mojang CDN.

What the SERVER saw (its log) is compared with what the manager says it did;
the files are compared with bytes the checker computes itself.

  A  200 files in parallel, SHA-1 checked; keep-alive: far fewer sockets than
     requests; no .part left
  B  3000 small files (the Minecraft assets case); a second run is all SKIPPED
     and sends no request at all
  C  resume: a connection cut at 300000 of 1000000 octets continues with
     `Range: bytes=300000-` and the whole file is right; a .part left by an
     earlier run; no digest and no validator -> no blind resume; a validator
     -> `If-Range`; a server that ignores Range; a 206 that starts elsewhere
  D  retries: 503 twice then fine (the backoff delays are within +-25 % of
     base and 2*base), Retry-After, a wrong digest twice then fine, a wrong
     digest for good (no dest, no part, old dest untouched), 404 (no retry),
     404 with a mirror, 500 for good
  E  ETag / Last-Modified: DL_META keeps a sidecar, the second run is a 304
     (zero octets), a changed file is replaced; validators given by the caller
  F  skip rules: right file skipped, wrong file of the right size replaced,
     DL_TRUST_SIZE, DL_FORCE
  G  cancel after 100000 octets: CANCELLED, part stays, the next run resumes
  H  rate limit 300 kB/s over 4 workers; parallel speed-up; a stalled read
     ends at the timeout
  I  chunked, redirect, empty file, gzip'd body, declared size wrong, bad
     scheme, parent is a file, connection refused, localhost by name, inline
     run, user agent, no Accept-Encoding
  J  (live, skipped without a route) Mojang: version manifest -> asset index
     -> 40 assets from resources.download.minecraft.net by SHA-1 and size,
     a resume of the client jar from a cut part
"""
import hashlib
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from fake_server import content  # noqa: E402

DL = sys.argv[1]
RUNNER = shlex.split(os.environ.get('RUNNER', ''))
FAILS = PASSES = SKIPS = 0

OK, SKIPPED, NOTMOD, FAILED, CANCELLED = 2, 3, 4, 5, 6
E_URL, E_HTTP, E_NET, E_HASH, E_SIZE, E_DISK, E_CANCEL, E_RANGE = 1, 2, 3, 4, 5, 6, 7, 8
F_NORESUME, F_META, F_TRUST, F_FORCE, F_NOSYNC = 1, 2, 4, 8, 16
SHA1, SHA256 = 2, 3


def ok(m):
    global PASSES
    PASSES += 1
    print('  OK    ' + m)


def bad(m, d=''):
    global FAILS
    FAILS += 1
    print('  FAIL  ' + m)
    if d:
        print('        ' + str(d)[:600].replace('\n', '\n        '))


def check(c, m, d=''):
    ok(m) if c else bad(m, d)


def skip(m):
    global SKIPS
    SKIPS += 1
    print('  SKIP  ' + m)


def sha(algo, data):
    return hashlib.new({2: 'sha1', 3: 'sha256'}[algo], data).hexdigest()


# ------------------------------------------------------------------ server
class Server:
    def __init__(self):
        self.p = subprocess.Popen([sys.executable, os.path.join(HERE, 'fake_server.py')],
                                  stdout=subprocess.PIPE)
        line = self.p.stdout.readline().decode().strip()
        m = re.match(r'port (\d+)', line)
        if not m:
            raise SystemExit('fake_server printed %r' % line)
        self.port = int(m.group(1))
        self.base = 'http://127.0.0.1:%d' % self.port

    def get(self, path):
        with urllib.request.urlopen(self.base + path, timeout=30) as r:
            return json.loads(r.read())

    def post(self, path):
        req = urllib.request.Request(self.base + path, data=b'', method='POST')
        urllib.request.urlopen(req, timeout=30).read()

    def reset(self):
        urllib.request.urlopen(self.base + '/reset', timeout=30).read()

    def stats(self):
        return self.get('/stats')

    def log(self):
        return self.get('/log')

    def stop(self):
        self.p.kill()


# ------------------------------------------------------------------ driver
class Report:
    pass


def run(spec, timeout=300, expect_rc=0):
    """spec: list of lines. Returns Report (the LAST X/I report)."""
    r = subprocess.run(RUNNER + [DL], input='\n'.join(spec) + '\n', capture_output=True,
                       text=True, timeout=timeout)
    rep = Report()
    rep.rc = r.returncode
    rep.raw = r.stdout
    rep.err = r.stderr
    rep.items = []
    rep.stats = {}
    rep.events = {}
    rep.delays = []
    rep.etag = {}
    rep.lm = {}
    rep.ms = 0
    rep.flags = []
    # keep only the last report (after the last '.' line)
    blocks = r.stdout.split('\n.\n')
    last = blocks[-2] if len(blocks) >= 2 else ''
    for ln in r.stdout.split('\n'):
        if ln in ('ADDFAIL', 'MIRRORFAIL', 'VALFAIL', 'RUNFAIL'):
            rep.flags.append(ln)
    for ln in last.split('\n'):
        w = ln.split(' ')
        if w[0] == 'ITEM':
            d = {'i': int(w[1])}
            for kv in w[2:]:
                k, v = kv.split('=')
                d[k] = int(v)
            rep.items.append(d)
        elif w[0] == 'STATS':
            rep.stats = {k: int(v) for k, v in (x.split('=') for x in w[1:])}
        elif w[0] == 'EVENTS':
            rep.events = {k: int(v) for k, v in (x.split('=') for x in w[1:])}
        elif w[0] == 'RETRYDELAYS':
            rep.delays = [int(x) for x in w[1:] if x]
        elif w[0] == 'ETAG':
            rep.etag[int(w[1])] = ' '.join(w[2:])
        elif w[0] == 'LM':
            rep.lm[int(w[1])] = ' '.join(w[2:])
        elif w[0] == 'MS':
            rep.ms = int(w[1])
    if expect_rc is not None and rep.rc != expect_rc:
        bad('driver exit %d' % rep.rc, rep.err + rep.raw[-300:])
    return rep


def A(url, dest, algo=0, hexd='-', size=0, flags=None):
    s = 'A %s %s %d %s %d' % (url, dest, algo, hexd, size)
    if flags is not None:
        s += ' %d' % flags
    return s


def readf(p):
    with open(p, 'rb') as f:
        return f.read()


def leftovers(d):
    out = []
    for root, _, files in os.walk(d):
        for f in files:
            if f.endswith('.part'):
                out.append(os.path.join(root, f))
    return out


def etag_of(name, size, v=0):
    return '"%s-%d-%d"' % (hashlib.md5(name.encode()).hexdigest()[:8], size, v)


# =================================================================== tests
def test_a(srv, tmp):
    print('== A. 200 files in parallel')
    srv.reset()
    d = os.path.join(tmp, 'a')
    spec = ['W 8']
    want = {}
    for i in range(200):
        size = 1000 + (i * 977) % 50000
        name = 'a%d' % i
        data = content(name, size)
        want[i] = (name, data)
        spec.append(A('%s/blob/%s?size=%d' % (srv.base, name, size), '%s/%s.bin' % (d, name),
                      SHA1, sha(SHA1, data), size))
    spec.append('X')
    t0 = time.time()
    rep = run(spec)
    el = time.time() - t0
    check(len(rep.items) == 200 and all(it['state'] == OK for it in rep.items),
          'all 200 OK (%.2fs)' % el, [it for it in rep.items if it['state'] != OK][:3])
    bad_files = [n for n, dta in want.values()
                 if not os.path.exists('%s/%s.bin' % (d, n)) or readf('%s/%s.bin' % (d, n)) != dta]
    check(not bad_files, 'every file has exactly the expected octets', bad_files[:3])
    check(not leftovers(d), 'no .part left')
    st = srv.stats()
    check(st['reqs'] == 200, 'the server saw 200 requests', st)
    check(rep.stats['sockets'] <= 10, 'keep-alive: %d sockets for 200 requests' % rep.stats['sockets'])
    check(rep.stats['reused'] >= 190, 'connections reused %d times' % rep.stats['reused'])
    check(st['conns'] <= 10, 'server: %d connections' % st['conns'])
    check(st['max_active'] <= 8, 'never more than 8 requests at once (%d)' % st['max_active'])
    total = sum(len(x[1]) for x in want.values())
    check(rep.stats['bytes_done'] == total and rep.stats['bytes_total'] == total and
          rep.stats['wire'] == total, 'byte counters agree (%d)' % total, rep.stats)
    check(rep.events.get('done') == 200 and rep.events.get('start') == 200, 'callback: 200 starts and dones', rep.events)


def test_b(srv, tmp):
    print('== B. 3000 assets')
    srv.reset()
    d = os.path.join(tmp, 'b')
    spec = ['W 16']
    want = {}
    for i in range(3000):
        size = 100 + (i * 7919) % 20000
        name = 'asset%d' % i
        data = content(name, size)
        h = sha(SHA1, data)
        want[name] = (h, data)
        spec.append(A('%s/blob/%s?size=%d' % (srv.base, name, size),
                      '%s/%s/%s' % (d, h[:2], h), SHA1, h, size))
    spec.append('X')
    t0 = time.time()
    rep = run(spec, timeout=600)
    el = time.time() - t0
    check(rep.stats.get('done') == 3000 and rep.stats.get('failed') == 0,
          '3000 files done in %.1fs (%.0f files/s)' % (el, 3000 / el), rep.stats)
    miss = [n for n, (h, dta) in want.items()
            if not os.path.exists('%s/%s/%s' % (d, h[:2], h)) or
            readf('%s/%s/%s' % (d, h[:2], h)) != dta]
    check(not miss, 'all 3000 files right', miss[:3])
    check(not leftovers(d), 'no .part left')
    check(rep.stats['sockets'] <= 20, '%d sockets for 3000 requests' % rep.stats['sockets'])
    print('  INFO  rate %.0f files/s, %d sockets' % (3000 / el, rep.stats['sockets']))
    # again: everything is there
    srv.reset()
    rep2 = run(spec, timeout=600)
    check(all(it['state'] == SKIPPED for it in rep2.items), 'second run: all 3000 SKIPPED')
    check(srv.stats()['reqs'] == 0, 'second run: not a single request')
    check(rep2.stats['wire'] == 0 and rep2.stats['skipped'] == 3000, 'second run: 0 octets on the wire', rep2.stats)
    print('  INFO  second run (hash check of 3000 files) %d ms' % rep2.ms)


def test_c(srv, tmp):
    print('== C. resume')
    srv.reset()
    d = os.path.join(tmp, 'c')
    os.makedirs(d)
    # C1 a cut connection continues
    size = 1000000
    data = content('cut1', size)
    rep = run(['W 2', 'O retries 3', 'O backoff 50 500',
               A('%s/cut/cut1?size=%d&at=300000&times=1' % (srv.base, size), d + '/cut1.bin',
                 SHA256, sha(SHA256, data), size), 'X'])
    it = rep.items[0]
    check(it['state'] == OK and it['attempts'] == 2, 'cut at 300000: OK after 2 attempts', it)
    check(readf(d + '/cut1.bin') == data, 'file is right')
    lg = [r for r in srv.log() if 'cut1' in r['path']]
    check(len(lg) == 2 and lg[0]['range'] is None and lg[1]['range'] == 'bytes=300000-',
          'second request: Range: bytes=300000-', lg)
    check(rep.stats['wire'] == size, 'octets on the wire: exactly %d (got %d)' % (size, rep.stats['wire']))
    check(rep.stats['bytes_done'] == size and rep.stats['bytes_total'] == size, 'progress counters', rep.stats)
    # C2 an old .part from an earlier run
    srv.reset()
    data = content('old1', size)
    with open(d + '/old1.bin.part', 'wb') as f:
        f.write(data[:400000])
    rep = run(['W 2', A('%s/blob/old1?size=%d' % (srv.base, size), d + '/old1.bin', SHA256, sha(SHA256, data), size), 'X'])
    it = rep.items[0]
    lg = srv.log()
    check(it['state'] == OK and it['attempts'] == 1 and readf(d + '/old1.bin') == data, 'old part + digest: OK in 1 attempt', it)
    check(lg and lg[0]['range'] == 'bytes=400000-', 'Range: bytes=400000-', lg)
    check(rep.stats['wire'] == 600000 and rep.stats['bytes_done'] == size,
          'wire 600000, done %d' % rep.stats['bytes_done'], rep.stats)
    # C3 a wrong prefix is caught by the digest and the file restarts
    srv.reset()
    data = content('old2', size)
    with open(d + '/old2.bin.part', 'wb') as f:
        f.write(b'\x00' * 400000)
    rep = run(['W 2', 'O retries 3', 'O backoff 20 100',
               A('%s/blob/old2?size=%d' % (srv.base, size), d + '/old2.bin', SHA256, sha(SHA256, data), size), 'X'])
    it = rep.items[0]
    lg = srv.log()
    check(it['state'] == OK and readf(d + '/old2.bin') == data and it['attempts'] == 2,
          'garbage prefix: digest fails, whole file again, OK', it)
    check(len(lg) == 2 and lg[1]['range'] is None, 'the retry asked for the whole file', lg)
    # C4 no digest, no validator: no blind resume
    srv.reset()
    data = content('blind', size)
    with open(d + '/blind.bin.part', 'wb') as f:
        f.write(data[:100000])
    rep = run(['W 1', A('%s/blob/blind?size=%d' % (srv.base, size), d + '/blind.bin', 0, '-', 0), 'X'])
    lg = srv.log()
    check(rep.items[0]['state'] == OK and readf(d + '/blind.bin') == data and lg[0]['range'] is None,
          'no digest, no validator: the part is NOT resumed', lg)
    # C5 validator (strong ETag) -> If-Range, resumes
    srv.reset()
    data = content('val1', size)
    with open(d + '/val1.bin.part', 'wb') as f:
        f.write(data[:100000])
    rep = run(['W 1', A('%s/blob/val1?size=%d' % (srv.base, size), d + '/val1.bin', 0, '-', 0),
               'V 0 %s -' % etag_of('val1', size), 'X'])
    lg = srv.log()
    check(rep.items[0]['state'] == OK and readf(d + '/val1.bin') == data, 'validator: OK, right octets', rep.items)
    check(lg and lg[0]['range'] == 'bytes=100000-' and lg[0]['if_range'] == etag_of('val1', size),
          'Range + If-Range with the ETag', lg)
    check(rep.stats['wire'] == size - 100000, 'only the rest crossed the wire (%d)' % rep.stats['wire'])
    # C6 validator that no longer matches -> the server sends everything (200)
    srv.reset()
    data = content('val2', size)
    with open(d + '/val2.bin.part', 'wb') as f:
        f.write(b'\x01' * 100000)
    rep = run(['W 1', A('%s/blob/val2?size=%d' % (srv.base, size), d + '/val2.bin', 0, '-', 0),
               'V 0 "stale-etag" -', 'X'])
    lg = srv.log()
    check(rep.items[0]['state'] == OK and readf(d + '/val2.bin') == data,
          'stale validator: server answers 200, file starts over, right octets', rep.items)
    check(lg[0]['if_range'] == '"stale-etag"' and rep.stats['wire'] == size, 'If-Range sent; whole file received')
    # C7 a server that ignores Range
    srv.reset()
    data = content('nor', size)
    with open(d + '/nor.bin.part', 'wb') as f:
        f.write(data[:200000])
    rep = run(['W 1', A('%s/norange/nor?size=%d' % (srv.base, size), d + '/nor.bin', SHA1, sha(SHA1, data), size), 'X'])
    check(rep.items[0]['state'] == OK and readf(d + '/nor.bin') == data and rep.stats['wire'] == size,
          'server ignores Range (200): the file starts over, right octets', rep.stats)
    check(rep.stats['bytes_done'] == size, 'progress not double counted (%d)' % rep.stats['bytes_done'])
    # C8 a 206 that starts elsewhere
    srv.reset()
    data = content('br', size)
    with open(d + '/br.bin.part', 'wb') as f:
        f.write(data[:200000])
    rep = run(['W 1', 'O retries 3', 'O backoff 10 50',
               A('%s/badrange/br?size=%d' % (srv.base, size), d + '/br.bin', SHA1, sha(SHA1, data), size), 'X'])
    it = rep.items[0]
    check(it['state'] == OK and readf(d + '/br.bin') == data and it['attempts'] == 2,
          'wrong Content-Range: refused, second attempt from zero', it)
    # C9 a complete part that was never renamed: no request
    srv.reset()
    data = content('whole', 50000)
    with open(d + '/whole.bin.part', 'wb') as f:
        f.write(data)
    rep = run(['W 1', A('%s/blob/whole?size=50000' % srv.base, d + '/whole.bin', SHA1, sha(SHA1, data), 50000), 'X'])
    check(rep.items[0]['state'] == OK and readf(d + '/whole.bin') == data and srv.stats()['reqs'] == 0,
          'complete part is verified and renamed without a request')
    check(not leftovers(d), 'no .part left')


def test_d(srv, tmp):
    print('== D. retries')
    srv.reset()
    d = os.path.join(tmp, 'd')
    os.makedirs(d)
    size = 20000
    # D1 503 twice
    data = content('fl1', size)
    rep = run(['W 1', 'O retries 4', 'O backoff 200 5000',
               A('%s/flaky/fl1?size=%d&fail=2' % (srv.base, size), d + '/fl1.bin', SHA1, sha(SHA1, data), size), 'X'])
    it = rep.items[0]
    check(it['state'] == OK and it['attempts'] == 3 and readf(d + '/fl1.bin') == data, '503 twice, then OK (3 attempts)', it)
    check(len(rep.delays) == 2 and 150 <= rep.delays[0] <= 250 and 300 <= rep.delays[1] <= 500,
          'backoff: 200 ms and 400 ms, +-25 %% (%s)' % rep.delays)
    check(rep.stats['retries'] == 2, 'retries counted', rep.stats)
    # D1b delays cap
    rep = run(['W 1', 'O retries 6', 'O backoff 100 250',
               A('%s/flaky/fl1b?size=%d&fail=5' % (srv.base, size), d + '/fl1b.bin', 0, '-', size), 'X'])
    check(rep.items[0]['state'] == OK and len(rep.delays) == 5 and max(rep.delays) <= 250 and rep.delays[-1] >= 150,
          'backoff doubles up to the cap (%s)' % rep.delays)
    # D2 Retry-After
    rep = run(['W 1', 'O retries 3', 'O backoff 50 5000',
               A('%s/flaky/fl2?size=%d&fail=1&ra=1' % (srv.base, size), d + '/fl2.bin', 0, '-', size), 'X'])
    check(rep.items[0]['state'] == OK and rep.delays and 1000 <= rep.delays[0] <= 5000,
          'Retry-After: 1 honoured (%s ms)' % rep.delays)
    # D3 wrong digest twice, then right
    data = content('co1', size)
    rep = run(['W 1', 'O retries 4', 'O backoff 10 50',
               A('%s/corrupt/co1?size=%d&times=2' % (srv.base, size), d + '/co1.bin', SHA1, sha(SHA1, data), size), 'X'])
    it = rep.items[0]
    check(it['state'] == OK and it['attempts'] == 3 and readf(d + '/co1.bin') == data,
          'wrong digest twice, then OK', it)
    check(rep.stats['bytes_done'] == size, 'no octet counted twice (%d)' % rep.stats['bytes_done'], rep.stats)
    # D4 wrong digest for good; the old dest stays
    with open(d + '/co2.bin', 'wb') as f:
        f.write(b'OLD CONTENT')
    data = content('co2', size)
    rep = run(['W 1', 'O retries 3', 'O backoff 10 50',
               A('%s/corrupt/co2?size=%d' % (srv.base, size), d + '/co2.bin', SHA1, sha(SHA1, data), size), 'X'])
    it = rep.items[0]
    check(it['state'] == FAILED and it['err'] == E_HASH and it['attempts'] == 3, 'wrong digest for good: FAILED/HASH after 3 attempts', it)
    check(readf(d + '/co2.bin') == b'OLD CONTENT', 'the old dest is untouched (atomic)')
    check(not os.path.exists(d + '/co2.bin.part'), 'the bad part is removed')
    # D5 404: no retry
    srv.reset()
    rep = run(['W 1', 'O retries 5', 'O backoff 10 50', A('%s/status/404' % srv.base, d + '/n404.bin', 0, '-', 0), 'X'])
    it = rep.items[0]
    check(it['state'] == FAILED and it['err'] == E_HTTP and it['http'] == 404 and it['attempts'] == 1,
          '404: FAILED/HTTP after one attempt', it)
    check(srv.stats()['reqs'] == 1 and not os.path.exists(d + '/n404.bin'), 'one request, no file')
    # D6 404 with a mirror
    srv.reset()
    data = content('mi1', size)
    rep = run(['W 1', 'O retries 3', 'O backoff 10 50',
               A('%s/status/404' % srv.base, d + '/mi1.bin', SHA1, sha(SHA1, data), size),
               'M 0 %s/blob/mi1?size=%d' % (srv.base, size), 'X'])
    it = rep.items[0]
    lg = srv.log()
    check(it['state'] == OK and it['attempts'] == 2 and readf(d + '/mi1.bin') == data, '404 then the mirror: OK', it)
    check(len(lg) == 2 and rep.delays[:1] in ([], [0]), 'the mirror came without a backoff (%s)' % rep.delays)
    # D7 500 for good
    srv.reset()
    rep = run(['W 1', 'O retries 3', 'O backoff 10 50', A('%s/status/500' % srv.base, d + '/n500.bin', 0, '-', 0), 'X'])
    it = rep.items[0]
    check(it['state'] == FAILED and it['http'] == 500 and it['attempts'] == 3 and srv.stats()['reqs'] == 3,
          '500: 3 attempts, FAILED/HTTP', it)
    # D8 mirrors, first one flaky for good, second one fine; order of requests
    srv.reset()
    data = content('mi2', size)
    rep = run(['W 1', 'O retries 2', 'O backoff 10 50',
               A('%s/status/503' % srv.base, d + '/mi2.bin', SHA1, sha(SHA1, data), size),
               'M 0 %s/blob/mi2?size=%d' % (srv.base, size), 'X'])
    paths = [r['path'] for r in srv.log()]
    check(rep.items[0]['state'] == OK and paths[0].startswith('/status/503') and paths[1].startswith('/blob/mi2'),
          '503 on the first URL: the next URL is tried at once', paths)
    # D9 connection refused
    rep = run(['W 1', 'O retries 2', 'O backoff 10 50', A('http://127.0.0.1:1/x', d + '/refused.bin', 0, '-', 0), 'X'])
    it = rep.items[0]
    check(it['state'] == FAILED and it['err'] == E_NET and it['attempts'] == 2, 'connection refused: FAILED/NET, 2 attempts', it)


def test_e(srv, tmp):
    print('== E. ETag / Last-Modified')
    srv.reset()
    d = os.path.join(tmp, 'e')
    os.makedirs(d)
    size = 30000
    url = '%s/mut/thing?size=%d' % (srv.base, size)
    rep = run(['W 1', A(url, d + '/thing.bin', 0, '-', 0, F_META), 'X'])
    check(rep.items[0]['state'] == OK and readf(d + '/thing.bin') == content('thing', size, 0), 'first run: OK')
    check(os.path.exists(d + '/thing.bin.dlmeta'), 'sidecar .dlmeta written')
    check(rep.etag.get(0) == etag_of('thing', size, 0), 'ETag reported: %s' % rep.etag.get(0))
    srv.reset()
    rep = run(['W 1', A(url, d + '/thing.bin', 0, '-', 0, F_META), 'X'])
    lg = srv.log()
    check(rep.items[0]['state'] == NOTMOD and rep.stats['wire'] == 0, 'second run: NOT_MODIFIED, 0 octets', rep.items)
    check(lg and lg[0]['inm'] == etag_of('thing', size, 0), 'If-None-Match sent', lg)
    srv.post('/bump/thing')
    rep = run(['W 1', A(url, d + '/thing.bin', 0, '-', 0, F_META), 'X'])
    check(rep.items[0]['state'] == OK and readf(d + '/thing.bin') == content('thing', size, 1),
          'after the server changed the file: replaced')
    check(rep.etag.get(0) == etag_of('thing', size, 1), 'new ETag kept', rep.etag)
    check(not leftovers(d), 'no .part left')
    # validators from the caller (no sidecar): Last-Modified only
    srv.reset()
    with open(d + '/lm.bin', 'wb') as f:
        f.write(content('lm', size, 0))
    lm = 'Tue,~14~Nov~2023~22:13:20~GMT'  # 1700000000
    rep = run(['W 1', A('%s/mut/lm?size=%d' % (srv.base, size), d + '/lm.bin', 0, '-', 0),
               'V 0 - %s' % lm, 'X'])
    lg = srv.log()
    check(rep.items[0]['state'] == NOTMOD and lg[0]['ims'] == lm.replace('~', ' '),
          'caller-supplied Last-Modified -> If-Modified-Since -> 304', (rep.items, lg))


def test_f(srv, tmp):
    print('== F. skip rules')
    srv.reset()
    d = os.path.join(tmp, 'f')
    os.makedirs(d)
    size = 10000
    data = content('sk', size)
    h = sha(SHA1, data)
    url = '%s/blob/sk?size=%d' % (srv.base, size)
    with open(d + '/right.bin', 'wb') as f:
        f.write(data)
    with open(d + '/wrong.bin', 'wb') as f:
        f.write(b'x' * size)
    with open(d + '/wrong2.bin', 'wb') as f:
        f.write(b'x' * size)
    with open(d + '/wrong3.bin', 'wb') as f:
        f.write(b'x' * size)
    with open(d + '/short.bin', 'wb') as f:
        f.write(data[:100])
    rep = run(['W 2', A(url, d + '/right.bin', SHA1, h, size),
               A(url, d + '/wrong.bin', SHA1, h, size),
               A(url, d + '/wrong2.bin', SHA1, h, size, F_TRUST),
               A(url, d + '/right.bin', SHA1, h, size, F_FORCE),
               A(url, d + '/short.bin', SHA1, h, size), 'X'])
    st = [it['state'] for it in rep.items]
    check(st == [SKIPPED, OK, SKIPPED, OK, OK], 'right skipped, wrong replaced, TRUST_SIZE trusts, FORCE fetches', st)
    check(readf(d + '/wrong.bin') == data and readf(d + '/short.bin') == data, 'replaced files are right')
    check(readf(d + '/wrong2.bin') == b'x' * size, 'TRUST_SIZE left the wrong file alone (documented)')
    check(srv.stats()['reqs'] == 3, 'only 3 requests were sent (%d)' % srv.stats()['reqs'])
    # size only, no digest
    srv.reset()
    rep = run(['W 1', A(url, d + '/wrong3.bin', 0, '-', size), 'X'])
    check(rep.items[0]['state'] == SKIPPED, 'no digest, right size: skipped')


def test_g(srv, tmp):
    print('== G. cancel and resume')
    srv.reset()
    d = os.path.join(tmp, 'g')
    os.makedirs(d)
    size = 600000
    data = content('cx', size)
    h = sha(SHA256, data)
    url = '%s/slow/cx?size=%d&ms=20&chunk=8192' % (srv.base, size)
    t0 = time.time()
    rep = run(['W 2', 'K 100000', A(url, d + '/cx.bin', SHA256, h, size),
               A(url.replace('cx', 'cy'), d + '/cy.bin', SHA256, sha(SHA256, content('cy', size)), size), 'X'])
    el = time.time() - t0
    states = [it['state'] for it in rep.items]
    check(all(s == CANCELLED for s in states) and all(it['err'] == E_CANCEL for it in rep.items),
          'both CANCELLED/CANCEL (%.2fs, far less than the 3 s of a full run)' % el, rep.items)
    check(not os.path.exists(d + '/cx.bin'), 'no dest after a cancel')
    sz = os.path.getsize(d + '/cx.bin.part') if os.path.exists(d + '/cx.bin.part') else -1
    check(0 < sz < size, 'the part stays (%d octets)' % sz)
    srv.reset()
    rep = run(['W 2', A(url, d + '/cx.bin', SHA256, h, size)])
    rep = run(['W 2', A(url, d + '/cx.bin', SHA256, h, size), 'X'])
    lg = srv.log()
    check(rep.items[0]['state'] == OK and readf(d + '/cx.bin') == data, 'next run: OK, right octets')
    check(lg and lg[0]['range'] == 'bytes=%d-' % sz, 'resumed with Range: bytes=%d-' % sz, lg[:1])


def test_h(srv, tmp):
    print('== H. rate limit, parallelism, timeout')
    srv.reset()
    d = os.path.join(tmp, 'h')
    os.makedirs(d)
    spec = ['W 4', 'O rate 300000']
    for i in range(6):
        spec.append(A('%s/blob/r%d?size=100000' % (srv.base, i), '%s/r%d.bin' % (d, i), 0, '-', 100000))
    spec.append('X')
    rep = run(spec)
    check(all(it['state'] == OK for it in rep.items), 'six files OK')
    check(1700 <= rep.ms <= 5000, '600 kB at 300 kB/s took %d ms (about 2000)' % rep.ms)
    # speed-up
    srv.reset()
    spec = ['W 8']
    for i in range(16):
        spec.append(A('%s/slow/p%d?size=8192&ms=300&chunk=4096' % (srv.base, i), '%s/p%d.bin' % (d, i), 0, '-', 8192))
    spec.append('X')
    rep = run(spec)
    st = srv.stats()
    check(all(it['state'] == OK for it in rep.items) and rep.ms < 16 * 600 * 0.45,
          '16 files of 0.6 s on 8 workers: %d ms (serial would be ~9600)' % rep.ms)
    check(6 <= st['max_active'] <= 8, 'the server saw %d requests at once' % st['max_active'])
    # stall -> timeout
    srv.reset()
    t0 = time.time()
    rep = run(['W 1', 'O retries 2', 'O backoff 10 50', 'O timeout 500',
               A('%s/stall/s1?size=100000&at=1000&ms=3000' % srv.base, d + '/s1.bin', 0, '-', 100000), 'X'])
    el = time.time() - t0
    it = rep.items[0]
    check(it['state'] == FAILED and it['err'] == E_NET and el < 5, 'a stalled read ends at the timeout (%.1fs)' % el, it)
    check(os.path.exists(d + '/s1.bin.part') and os.path.getsize(d + '/s1.bin.part') >= 1000,
          'the part with what arrived stays')


def test_i(srv, tmp):
    print('== I. odds and ends')
    srv.reset()
    d = os.path.join(tmp, 'i')
    os.makedirs(d)
    size = 100000
    data = content('ch', size)
    rep = run(['W 1', A('%s/chunked/ch?size=%d' % (srv.base, size), d + '/ch.bin', SHA1, sha(SHA1, data), size), 'X'])
    check(rep.items[0]['state'] == OK and readf(d + '/ch.bin') == data, 'chunked body')
    data = content('rd', size)
    rep = run(['W 1', A('%s/redir/rd?size=%d' % (srv.base, size), d + '/rd.bin', SHA1, sha(SHA1, data), size), 'X'])
    check(rep.items[0]['state'] == OK and readf(d + '/rd.bin') == data, 'redirect 302')
    rep = run(['W 1', A('%s/blob/empty?size=0' % srv.base, d + '/empty.bin', 0, '-', 0), 'X'])
    check(rep.items[0]['state'] == OK and os.path.exists(d + '/empty.bin') and os.path.getsize(d + '/empty.bin') == 0,
          'empty file')
    data = content('gz', size)
    rep = run(['W 1', A('%s/gz/gz?size=%d' % (srv.base, size), d + '/gz.bin', SHA1, sha(SHA1, data), size), 'X'])
    check(rep.items[0]['state'] == OK and readf(d + '/gz.bin') == data, 'Content-Encoding: gzip sent anyway: decoded (D8)')
    rep = run(['W 1', 'O retries 2', 'O backoff 10 50', A('%s/blob/sz?size=%d' % (srv.base, size), d + '/sz.bin', 0, '-', size + 5), 'X'])
    it = rep.items[0]
    check(it['state'] == FAILED and it['err'] == E_SIZE and not os.path.exists(d + '/sz.bin'),
          'declared size differs from what the server sends: FAILED/SIZE, no file', it)
    rep = run(['W 1', 'O retries 2', 'O backoff 10 50', A('ftp://x/y', d + '/ftp.bin', 0, '-', 0), 'X'])
    it = rep.items[0]
    check(it['state'] == FAILED and it['err'] == E_URL and it['attempts'] == 1, 'ftp:// -> FAILED/URL, no retry', it)
    with open(d + '/afile', 'wb') as f:
        f.write(b'x')
    rep = run(['W 1', A('%s/blob/pf?size=10' % srv.base, d + '/afile/sub/x.bin', 0, '-', 0), 'X'])
    check(rep.items[0]['state'] == FAILED and rep.items[0]['err'] == E_DISK, 'parent is a file: FAILED/DISK', rep.items)
    rep = run(['W 1', 'O retries 2', 'O backoff 10 50',
               A('%s/blob/nest?size=77' % srv.base, d + '/a/b/c/d/nest.bin', 0, '-', 0), 'X'])
    check(rep.items[0]['state'] == OK and os.path.getsize(d + '/a/b/c/d/nest.bin') == 77, 'nested directories are made')
    rep = run(['W 1', A('http://localhost:%d/blob/lh?size=500' % srv.port, d + '/lh.bin', 0, '-', 0), 'X'])
    check(rep.items[0]['state'] == OK, 'a host NAME (localhost)', rep.items)
    srv.reset()
    rep = run(['W 1', 'O ua Firn-Test/1', A('%s/blob/ua?size=10' % srv.base, d + '/ua.bin', 0, '-', 0), 'I'])
    lg = srv.log()
    check(rep.items[0]['state'] == OK and lg[0]['ua'] == 'Firn-Test/1', 'inline run; User-Agent set', lg)
    check(lg[0]['ae'] is None, 'no Accept-Encoding sent')
    rep = run(['W 1', 'ADD'])
    rep = run(['W 1', A('http://x/y', d + '/bad.bin', SHA1, 'abc', 0), 'X'], expect_rc=None)
    check('ADDFAIL' in rep.flags, 'a digest of the wrong length is refused at dl_add')
    rep = run(['W 1', A('http://x/y', d + '/bad.bin', SHA1, 'zz' * 20, 0), 'X'], expect_rc=None)
    check('ADDFAIL' in rep.flags, 'a digest that is not hex is refused at dl_add')


# ------------------------------------------------------------------- live
def test_j(tmp):
    print('== J. live: Mojang')
    try:
        def fetch(u):
            with urllib.request.urlopen(u, timeout=20) as r:
                return r.read()
        man = json.loads(fetch('https://piston-meta.mojang.com/mc/game/version_manifest_v2.json'))
    except Exception as e:  # noqa
        skip('no route to piston-meta.mojang.com (%s)' % str(e)[:60])
        return
    rel = man['latest']['release']
    ver = next(v for v in man['versions'] if v['id'] == rel)
    vj = json.loads(fetch(ver['url']))
    ai = vj['assetIndex']
    d = os.path.join(tmp, 'j')
    os.makedirs(d)
    rep = run(['W 4', A(ai['url'], d + '/index.json', SHA1, ai['sha1'], ai['size']), 'X'])
    check(rep.items[0]['state'] == OK and sha(SHA1, readf(d + '/index.json')) == ai['sha1'],
          'asset index %s (https, SHA-1 and size)' % ai['id'], rep.items)
    idx = json.loads(readf(d + '/index.json'))['objects']
    names = sorted(idx)[:40]
    spec = ['W 8']
    for n in names:
        o = idx[n]
        h = o['hash']
        spec.append(A('https://resources.download.minecraft.net/%s/%s' % (h[:2], h),
                      '%s/objects/%s/%s' % (d, h[:2], h), SHA1, h, o['size']))
    spec.append('X')
    rep = run(spec)
    check(all(it['state'] == OK for it in rep.items), '40 assets by hash and size (https, keep-alive)', [it for it in rep.items if it['state'] != OK][:2])
    print('  INFO  %d sockets for %d requests, %d ms' % (rep.stats['sockets'], rep.stats['requests'], rep.ms))
    check(rep.stats['sockets'] < 40, 'connections were reused (%d sockets)' % rep.stats['sockets'])
    # resume the client jar from a cut part
    cl = vj['downloads']['client']
    part = d + '/client.jar.part'
    try:
        req = urllib.request.Request(cl['url'], headers={'Range': 'bytes=0-4999999'})
        with urllib.request.urlopen(req, timeout=60) as r:
            first = r.read()
        check(len(first) == 5000000, 'the CDN honours Range (206)')
        with open(part, 'wb') as f:
            f.write(first)
    except Exception as e:  # noqa
        skip('Range against the CDN failed: %s' % str(e)[:80])
        return
    rep = run(['W 1', A(cl['url'], d + '/client.jar', SHA1, cl['sha1'], cl['size']), 'X'], timeout=600)
    it = rep.items[0]
    check(it['state'] == OK and sha(SHA1, readf(d + '/client.jar')) == cl['sha1'],
          'client.jar resumed from 5 MB, SHA-1 right (%d octets on the wire)' % rep.stats['wire'], it)
    check(rep.stats['wire'] == cl['size'] - 5000000, 'only the missing %d octets were fetched' % (cl['size'] - 5000000))


# ------------------------------------------------------------------- main
def main():
    tmp = tempfile.mkdtemp(prefix='dlcheck.')
    srv = Server()
    only = set(sys.argv[2:])
    try:
        for name, fn in (('a', test_a), ('b', test_b), ('c', test_c), ('d', test_d), ('e', test_e),
                         ('f', test_f), ('g', test_g), ('h', test_h), ('i', test_i)):
            if not only or name in only:
                fn(srv, tmp)
        if not only or 'j' in only:
            test_j(tmp)
    finally:
        srv.stop()
        shutil.rmtree(tmp, ignore_errors=True)
    print('\n%d passed, %d failed, %d skipped' % (PASSES, FAILS, SKIPS))
    return 1 if FAILS else 0


if __name__ == '__main__':
    sys.exit(main())
