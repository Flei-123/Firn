#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/oauth/check_msa.py <msa_main> [sections] -- lib/auth/msa.fi (the
Microsoft -> Xbox Live -> XSTS -> Minecraft services chain) against
tools/oauth/fake_msa.py, which checks every header and every field of every
request body, and, when there is a route, against the real hosts with
credentials that cannot work (the refusals are the test).

  A  request bodies and small helpers: exact JSON of the three bodies,
     the XErr texts, the dashed UUID
  B  the whole chain by device code: order of the requests, headers, tokens,
     profile, expiry, ownership
  C  XSTS refusals (XErr): no Xbox account, child account, unknown number
  D  Minecraft services refuse the Xbox token; HTTP errors; a body of the
     wrong shape
  E  ownership: a profile without entitlements (Game Pass), entitlements
     without a profile, neither
  F  ensure/refresh: nothing sent while the token is good; the chain again
     when only the Minecraft token ran out; a refresh when the Microsoft
     token ran out (and the refresh token rotates)
  G  the keyring: save, load in a new process, nothing readable on disk
  H  a wrong client id; cancel while waiting for the user
  L  (live, skipped without a route) login.microsoftonline.com,
     user.auth.xboxlive.com, xsts.auth.xboxlive.com, api.minecraftservices.com
     answer a bogus client / bogus tokens the way the protocol says -- which
     proves TLS, HTTP and the JSON handling against the real services. No
     real login is attempted: there is no registered client id here.
"""
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request
import uuid

HERE = os.path.dirname(os.path.abspath(__file__))
DRV = sys.argv[1]
RUNNER = shlex.split(os.environ.get('RUNNER', ''))
FAILS = PASSES = SKIPS = 0
UUID = '069a79f444e94726a5befca90e38aaf5'


def ok(m):
    global PASSES
    PASSES += 1
    print('  OK    ' + m)


def bad(m, d=''):
    global FAILS
    FAILS += 1
    print('  FAIL  ' + m)
    if d:
        print('        ' + str(d)[:500].replace('\n', '\n        '))


def check(c, m, d=''):
    ok(m) if c else bad(m, d)


def skip(m):
    global SKIPS
    SKIPS += 1
    print('  SKIP  ' + m)


class Srv:
    def __init__(self):
        self.p = subprocess.Popen([sys.executable, os.path.join(HERE, 'fake_msa.py')], stdout=subprocess.PIPE)
        line = self.p.stdout.readline().decode().strip()
        m = re.match(r'port (\d+)', line)
        if not m:
            raise SystemExit('fake_msa printed %r' % line)
        self.base = 'http://127.0.0.1:%s' % m.group(1)

    def call(self, path, body=None):
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(self.base + path, data=data, method='POST' if data is not None else 'GET')
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.loads(r.read() or b'{}')

    def knobs(self, **kw):
        self.call('/knobs', kw)

    def reset(self):
        self.call('/reset', {})

    def log(self):
        return self.call('/log')

    def paths(self):
        return [x['path'] for x in self.log()]

    def stop(self):
        self.p.kill()


class Out:
    def __init__(self, lines):
        self.lines = lines
        self.result = None
        self.name = ''
        self.step = ''
        self.xerr = 0
        self.http = 0
        self.net = 0
        self.err = ''
        self.desc = ''
        self.session = {}
        self.ms = {}
        for ln in lines:
            if ln.startswith('RESULT '):
                m = re.match(r'RESULT (-?\d+) (\S+) step=(\S*) xerr=(\d+) http=(\d+) net=(\d+) err=(\S*) desc=(.*)$', ln)
                self.result, self.name, self.step = int(m.group(1)), m.group(2), m.group(3)
                self.xerr, self.http, self.net = int(m.group(4)), int(m.group(5)), int(m.group(6))
                self.err, self.desc = m.group(7), m.group(8)
            elif ln.startswith('SESSION '):
                self.session = dict(re.findall(r'(\w+)=(\S*)', ln))
            elif ln.startswith('MS '):
                self.ms = dict(re.findall(r'(\w+)=(\S*)', ln))

    def line(self, prefix):
        for ln in self.lines:
            if ln.startswith(prefix):
                return ln[len(prefix):]
        return None


class Session:
    def __init__(self, env=None):
        e = dict(os.environ)
        e.update(env or {})
        self.p = subprocess.Popen(RUNNER + [DRV], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, text=True, bufsize=1, env=e)

    def cmd(self, line, timeout=120, on_line=None):
        self.p.stdin.write(line + '\n')
        self.p.stdin.flush()
        lines = []
        while True:
            ln = self.p.stdout.readline()
            if ln == '':
                raise RuntimeError('driver ended; stderr: ' + self.p.stderr.read()[-300:])
            ln = ln.rstrip('\n')
            if ln == '.':
                break
            lines.append(ln)
            if on_line:
                on_line(ln)
        return Out(lines)

    def close(self):
        try:
            self.p.stdin.write('QUIT\n')
            self.p.stdin.flush()
            self.p.wait(timeout=10)
        except Exception:
            self.p.kill()


def device_login(s, srv, after=1.2):
    def approve(ln):
        if ln.startswith('DEVICE '):
            threading.Timer(after, lambda: srv.call('/approve')).start()
    return s.cmd('DEVICE', timeout=60, on_line=approve)


def fresh(srv, **knobs):
    srv.reset()
    if knobs:
        srv.knobs(**knobs)
    s = Session()
    s.cmd('CFG base ' + srv.base)
    return s


def test_a(srv, tmp):
    print('== A. bodies and helpers')
    s = Session()
    r = s.cmd('BODY xbl TOK.en-1_x')
    b = json.loads(r.line('BODY '))
    check(b == {'Properties': {'AuthMethod': 'RPS', 'SiteName': 'user.auth.xboxlive.com', 'RpsTicket': 'd=TOK.en-1_x'},
                'RelyingParty': 'http://auth.xboxlive.com', 'TokenType': 'JWT'}, 'Xbox Live body', b)
    r = s.cmd('BODY xsts XBLTOKEN')
    b = json.loads(r.line('BODY '))
    check(b == {'Properties': {'SandboxId': 'RETAIL', 'UserTokens': ['XBLTOKEN']},
                'RelyingParty': 'rp://api.minecraftservices.com/', 'TokenType': 'JWT'}, 'XSTS body', b)
    r = s.cmd('BODY mc 1234567 XSTS.tok')
    check(json.loads(r.line('BODY ')) == {'identityToken': 'XBL3.0 x=1234567;XSTS.tok'}, 'login_with_xbox body')
    r = s.cmd('UUID ' + UUID)
    check(r.line('UUID ') == '069a79f4-44e9-4726-a5be-fca90e38aaf5', 'uuid with dashes')
    r = s.cmd('UUID short')
    check(r.line('UUID ') == 'short', 'a string that is no uuid is left alone')
    for x, want in ((2148916233, 'no Xbox account'), (2148916238, 'child account'), (2148916235, 'country'),
                    (2148916236, 'adult verification'), (2148916227, 'banned')):
        r = s.cmd('XERR %d' % x)
        check(want in r.line('XERR '), 'XErr %d: %s' % (x, r.line('XERR ')[:50]))
    r = s.cmd('XERR 12345')
    check(r.line('XERR ') == '', 'an unknown number has no text')
    s.close()


def test_b(srv, tmp):
    print('== B. the chain by device code')
    s = fresh(srv)
    t0 = time.time()
    r = device_login(s, srv)
    check(r.result == 0, 'login ok (%.1fs)' % (time.time() - t0), r.lines)
    ss = r.session
    check(ss['ok'] == '1' and ss['uuid'] == UUID and ss['name'] == 'Steve_42' and ss['owns'] == '1',
          'profile: uuid, name, ownership', ss)
    check(ss['mc'].startswith('MC-') and 86000 < int(ss['exp']) <= 86400, 'Minecraft token, expires in %ss' % ss['exp'])
    check(ss['uhs'].startswith('UHS-XSTS-') or ss['uhs'].startswith('UHS-'), 'user hash from the XSTS answer: %s' % ss['uhs'])
    check(r.ms['access'].startswith('MSAT-') and r.ms['refresh'].startswith('MSRT-'), 'Microsoft tokens kept', r.ms)
    lg = srv.log()
    paths = [x['path'] for x in lg if x['path'] != '/log']
    check(paths[0] == '/devicecode' and paths[1:-5] == ['/token'] * (len(paths) - 6) and
          paths[-5:] == ['/xbl/user/authenticate', '/xsts/authorize', '/mc/authentication/login_with_xbox',
                         '/mc/entitlements/mcstore', '/mc/minecraft/profile'],
          'the order: devicecode, token, xbl, xsts, minecraft, entitlements, profile', paths)
    dev = [x for x in lg if x['path'] == '/devicecode'][0]
    check(dev['form'] == {'client_id': 'azure-client-id', 'scope': 'XboxLive.signin offline_access'},
          'device request: client id and scope', dev['form'])
    for p in ('/xbl/user/authenticate', '/xsts/authorize', '/mc/authentication/login_with_xbox'):
        x = [q for q in lg if q['path'] == p][0]
        check((x['ctype'] or '').startswith('application/json') and (x['accept'] or '') == 'application/json',
              '%s: Content-Type and Accept are application/json' % p)
    ent = [q for q in lg if q['path'] == '/mc/entitlements/mcstore'][0]
    check(ent['auth'] == 'Bearer ' + ss['mc'] and ent['token_known'], 'entitlements: Bearer with the Minecraft token')
    pr = [q for q in lg if q['path'] == '/mc/minecraft/profile'][0]
    check(pr['auth'] == 'Bearer ' + ss['mc'], 'profile: Bearer with the Minecraft token')
    xbl = [q for q in lg if q['path'] == '/xbl/user/authenticate'][0]
    check(xbl['body']['Properties']['RpsTicket'] == 'd=' + r.ms['access'], 'the RPS ticket is d=<Microsoft access token>')
    s.close()


def test_c(srv, tmp):
    print('== C. XSTS refusals')
    for xerr, text in ((2148916233, 'no Xbox account'), (2148916238, 'child'), (2148916235, 'country'), (2148916999, '')):
        s = fresh(srv, xerr=xerr)
        r = device_login(s, srv, 0.3)
        check(r.result == 101 and r.xerr == xerr and r.step == 'xsts', 'XErr %d -> MSA_XERR at the xsts step' % xerr, r.lines)
        check(r.session['ok'] == '0' and r.session['mc'] == '', '... no Minecraft token handed out')
        t = s.cmd('XERR %d' % xerr).line('XERR ')
        check((text in t) if text else t == '', '... %s' % (t[:60] or 'no text for an unknown number'))
        check('/mc/authentication/login_with_xbox' not in srv.paths(), '... the chain stopped there')
        s.close()


def test_d(srv, tmp):
    print('== D. Minecraft services refuse; HTTP errors; odd bodies')
    s = fresh(srv, mc_refuse=True)
    r = device_login(s, srv, 0.3)
    check(r.result == 104 and r.step == 'minecraft', 'login_with_xbox refused: MSA_MC_REFUSED', r.lines)
    s.close()
    s = fresh(srv, xbl_status=503)
    r = device_login(s, srv, 0.3)
    check(r.result == 2 and r.step == 'xbl' and r.http == 503, 'Xbox Live 503: OA_HTTP at xbl', r.lines)
    s.close()
    s = fresh(srv, bad_xbl_shape=True)
    r = device_login(s, srv, 0.3)
    check(r.result == 3 and r.step == 'xbl', 'an answer without DisplayClaims: OA_PROTOCOL', r.lines)
    s.close()


def test_e(srv, tmp):
    print('== E. ownership')
    s = fresh(srv, owns=False, has_profile=True)
    r = device_login(s, srv, 0.3)
    check(r.result == 0 and r.session['owns'] == '0' and r.session['uuid'] == UUID,
          'a profile without entitlements (Game Pass): ok, owns=0 says what the entitlements said', r.lines)
    s.close()
    s = fresh(srv, owns=True, has_profile=False)
    r = device_login(s, srv, 0.3)
    check(r.result == 103 and r.step == 'profile', 'entitlements but no profile: MSA_NO_PROFILE', r.lines)
    s.close()
    s = fresh(srv, owns=False, has_profile=False)
    r = device_login(s, srv, 0.3)
    check(r.result == 102, 'neither: MSA_NOT_OWNED', r.lines)
    s.close()


def test_f(srv, tmp):
    print('== F. ensure and refresh')
    s = fresh(srv)
    r = device_login(s, srv, 0.3)
    mc1, ms1 = r.session['mc'], r.ms['refresh']
    n0 = len(srv.log())
    r = s.cmd('ENSURE 60')
    check(r.result == 0 and r.session['mc'] == mc1 and len(srv.log()) == n0,
          'ensure while the token is good: no request', r.lines)
    n0 = len(srv.paths())
    r = s.cmd('ENSURE 99999')
    new = srv.paths()[n0:]
    check(r.result == 0 and r.session['mc'] != mc1 and new[0] == '/xbl/user/authenticate' and '/token' not in new,
          'the Minecraft token ran out, the Microsoft one is good: the chain again, no /token', new)
    s.close()
    # the Microsoft token ran out too: a refresh
    srv.reset()
    srv.knobs(ms_expires=1, mc_expires=1)
    s = Session()
    s.cmd('CFG base ' + srv.base)
    r = device_login(s, srv, 0.3)
    check(r.result == 0, 'login with 1 s tokens', r.lines)
    old_refresh = r.ms['refresh']
    time.sleep(1.2)
    srv.knobs(ms_expires=3600, mc_expires=86400)
    n0 = len(srv.paths())
    r = s.cmd('ENSURE 0')
    new = srv.paths()[n0:]
    check(r.result == 0 and new[0] == '/token' and new[1] == '/xbl/user/authenticate', 'ensure: refresh first, then the chain', new)
    check(r.ms['refresh'] != old_refresh and r.session['ok'] == '1', 'the refresh token rotated; session ok')
    tok = [x for x in srv.log() if x['path'] == '/token'][-1]
    check(tok['form'].get('grant_type') == 'refresh_token' and tok['form'].get('refresh_token') == old_refresh and
          tok['form'].get('client_id') == 'azure-client-id', 'the refresh request', tok['form'])
    # a refresh token that is no longer good
    r2 = s.cmd('CLEAR')
    s.cmd('LOAD none none')
    r = s.cmd('REFRESH')
    check(r.result == 16, 'refresh without tokens: OA_NOTOKEN', r.lines)
    s.close()


def test_g(srv, tmp):
    print('== G. keyring')
    d = os.path.join(tmp, 'xdg_msa')
    os.makedirs(d)
    env = {'XDG_DATA_HOME': d}
    srv.reset()
    s = Session(env=env)
    s.cmd('CFG base ' + srv.base)
    r = device_login(s, srv, 0.3)
    mc, ref = r.session['mc'], r.ms['refresh']
    r = s.cmd('SAVE msatest main')
    check(r.line('STORE ') == '1', 'saved')
    s.close()
    s2 = Session(env=env)
    s2.cmd('CFG base ' + srv.base)
    r = s2.cmd('LOAD msatest main')
    check(r.line('STORE ') == '1' and r.session['uuid'] == UUID and r.session['name'] == 'Steve_42' and
          r.session['mc'] == mc and r.ms['refresh'] == ref and r.session['owns'] == '1',
          'loaded in a new process: identical session')
    blob = b''
    for root, _, files in os.walk(d):
        for f in files:
            blob += open(os.path.join(root, f), 'rb').read()
    check(mc.encode() not in blob and ref.encode() not in blob and len(blob) > 100, 'nothing readable on disk')
    r = s2.cmd('ENSURE 60')
    check(r.result == 0, 'a loaded session is usable (ensure sends nothing while good)')
    r = s2.cmd('DEL msatest main')
    r = s2.cmd('LOAD msatest main')
    check(r.line('STORE ') == '0', 'deleted')
    s2.close()


def test_h(srv, tmp):
    print('== H. wrong client id, cancel')
    srv.reset()
    s = Session()
    s.cmd('CFG base ' + srv.base)
    s.cmd('CFG client_id nope')
    r = s.cmd('DEVICE_START')
    check(r.result == 14 and r.err == 'unauthorized_client' and 'AADSTS700016' in r.desc,
          'unknown application: the OAuth error is passed on', r.lines)
    s.close()
    if os.environ.get('RUNNER'):
        skip('cancel flag: the Windows runtime has no threads to set it')
        return
    s = fresh(srv)
    s.cmd('CANCELIN 1500')
    t0 = time.time()
    r = s.cmd('DEVICE', timeout=30)
    check(r.result == 8 and time.time() - t0 < 6, 'cancel while waiting for the user: OA_CANCELLED', r.lines)
    s.close()


def test_l(tmp):
    print('== L. live: the real services, bogus credentials')
    try:
        urllib.request.urlopen('https://login.microsoftonline.com/consumers/v2.0/.well-known/openid-configuration', timeout=15).read()
    except Exception as e:  # noqa
        skip('no route to login.microsoftonline.com (%s)' % str(e)[:60])
        return
    s = Session()
    r = s.cmd('LIVE_DEVICE ' + str(uuid.uuid4()))
    check(r.result == 14 and r.net == 0 and r.err != '', 'login.microsoftonline.com: a made-up client id is refused with an OAuth error (%s, HTTP %d)' % (r.err, r.http), r.lines)
    r = s.cmd('LIVE_XBL')
    check(r.net == 0 and r.http in (400, 401) and r.result in (2, 3), 'user.auth.xboxlive.com: a bogus RPS ticket is refused (HTTP %d)' % r.http, r.lines)
    r = s.cmd('LIVE_XSTS')
    check(r.net == 0 and r.http in (400, 401) and r.result in (2, 3, 101), 'xsts.auth.xboxlive.com: a bogus user token is refused (HTTP %d, result %s)' % (r.http, r.name), r.lines)
    r = s.cmd('LIVE_MC')
    check(r.net == 0 and r.result == 104 or (r.net == 0 and r.http in (400, 401, 403)), 'api.minecraftservices.com: a bogus identity token is refused (HTTP %d, %s)' % (r.http, r.name), r.lines)
    s.close()


def main():
    tmp = tempfile.mkdtemp(prefix='msacheck.')
    srv = Srv()
    only = set(sys.argv[2:])
    try:
        for name, fn in (('a', test_a), ('b', test_b), ('c', test_c), ('d', test_d), ('e', test_e),
                         ('f', test_f), ('g', test_g), ('h', test_h)):
            if not only or name in only:
                fn(srv, tmp)
        if not only or 'l' in only:
            test_l(tmp)
    finally:
        srv.stop()
        shutil.rmtree(tmp, ignore_errors=True)
    print('\n%d passed, %d failed, %d skipped' % (PASSES, FAILS, SKIPS))
    return 1 if FAILS else 0


if __name__ == '__main__':
    sys.exit(main())
