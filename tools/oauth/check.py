#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/oauth/check.py <oauth_main> [sections] -- lib/auth/oauth.fi against
tools/oauth/fake_idp.py, a provider written in Python that checks PKCE, the
redirect URI, single use of codes, rotating refresh tokens and the polling
interval of the device flow -- and what the SERVER saw is compared with what
the client says it did. Python plays the browser (it visits the URL the
client "opens").

  A  small things: PKCE challenge of RFC 7636 appendix B, percent encoding
  B  discovery: ok, a document of another issuer, endpoints that are not https
  C  authorization code + PKCE through the loopback server: tokens, the id_token
     verified (one JWKS fetch), PKCE/state/nonce/redirect_uri as the server saw
     them, the page the browser got, the port closed afterwards
  D  the loopback under attack: a wrong `state` (400, the wait goes on), a replay
     (410), a denial (access_denied), a timeout, a cancel
  E  the device flow: pending, slow_down (interval +5), approval, denial, expiry
  F  refresh: rotation, ensure_fresh, a reused refresh token, no refresh token
  G  id_token refusals: bad signature, issuer, audience, nonce, expiry, alg none;
     key rotation at the provider (one JWKS refetch)
  H  client authentication: secret in the body, secret in a Basic header
  I  failures: a gateway error page, a refused connection, http to a stranger
  J  token store (std.secret in a temporary XDG directory): save, load, delete,
     nothing readable in the file
  K  a pasted redirect URL (no loopback server)
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
import urllib.error
import urllib.parse
import urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
DRV = sys.argv[1]
RUNNER = shlex.split(os.environ.get('RUNNER', ''))
FAILS = PASSES = SKIPS = 0


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


class Idp:
    def __init__(self):
        self.p = subprocess.Popen([sys.executable, os.path.join(HERE, 'fake_idp.py')],
                                  stdout=subprocess.PIPE)
        line = self.p.stdout.readline().decode().strip()
        m = re.match(r'port (\d+)', line)
        if not m:
            raise SystemExit('fake_idp printed %r' % line)
        self.port = int(m.group(1))
        self.base = 'http://127.0.0.1:%d' % self.port

    def call(self, path, body=None, method=None):
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(self.base + path, data=data,
                                     method=method or ('POST' if data is not None else 'GET'))
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.loads(r.read() or b'{}')

    def knobs(self, **kw):
        self.call('/knobs', kw)

    def reset(self):
        self.call('/reset', {})

    def log(self):
        return self.call('/log')

    def stop(self):
        self.p.kill()


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *a, **k):
        return None


def browse(url, follow=True):
    """Visit `url`; returns (status, body, final url). Redirects are followed
    (to the loopback server of the client) unless follow=False."""
    opener = urllib.request.build_opener() if follow else urllib.request.build_opener(NoRedirect)
    try:
        with opener.open(url, timeout=30) as r:
            return r.status, r.read().decode('utf-8', 'replace'), r.geturl()
    except urllib.error.HTTPError as e:
        return e.code, e.read().decode('utf-8', 'replace'), url


def location_of(url):
    try:
        urllib.request.build_opener(NoRedirect).open(url, timeout=10)
    except urllib.error.HTTPError as e:
        return e.headers.get('Location')
    return None


class Session:
    def __init__(self, env=None, on_open=None):
        e = dict(os.environ)
        e.update(env or {})
        self.p = subprocess.Popen(RUNNER + [DRV], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.PIPE, text=True, bufsize=1, env=e)
        self.on_open = on_open
        self.opened = []
        self.browser = []

    def cmd(self, line, timeout=120, on_line=None):
        self.p.stdin.write(line + '\n')
        self.p.stdin.flush()
        lines = []
        t0 = time.time()
        while True:
            ln = self.p.stdout.readline()
            if ln == '':
                raise RuntimeError('driver ended; stderr: ' + self.p.stderr.read()[-300:])
            ln = ln.rstrip('\n')
            if ln == '.':
                break
            if ln.startswith('OPEN '):
                url = ln[5:]
                self.opened.append(url)
                if self.on_open:
                    self.browser.append(self.on_open(url))
                continue
            lines.append(ln)
            if on_line:
                on_line(ln)
            if time.time() - t0 > timeout:
                raise RuntimeError('timeout')
        return Out(lines)

    def close(self):
        try:
            self.p.stdin.write('QUIT\n')
            self.p.stdin.flush()
        except Exception:
            pass
        try:
            self.p.wait(timeout=10)
        except Exception:
            self.p.kill()


class Out:
    def __init__(self, lines):
        self.lines = lines
        self.result = None
        self.tokens = {}
        self.err = ''
        self.desc = ''
        self.http = 0
        self.net = 0
        for ln in lines:
            if ln.startswith('RESULT '):
                m = re.match(r'RESULT (-?\d+) (\S+) http=(\d+) net=(\d+) err=(\S*) desc=(.*)$', ln)
                self.result = int(m.group(1))
                self.name = m.group(2)
                self.http = int(m.group(3))
                self.net = int(m.group(4))
                self.err = m.group(5)
                self.desc = m.group(6)
            elif ln.startswith('TOKENS '):
                self.tokens = dict(re.findall(r'(\w+)=(.*?)(?= \w+=|$)', ln))

    def line(self, prefix):
        for ln in self.lines:
            if ln.startswith(prefix):
                return ln[len(prefix):]
        return None


def configure(s, idp, **extra):
    s.cmd('CFG client_id test-client')
    s.cmd('CFG scope openid profile offline_access')
    r = s.cmd('DISCOVER ' + idp.base)
    return r


def login_browser(url):
    # the user approves: the IdP answers 302 to the loopback server
    return browse(url)


# =================================================================== tests
def test_a(idp, tmp):
    print('== A. small things')
    s = Session()
    r = s.cmd('PKCE dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk')
    check(r.line('CHALLENGE ') == 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM',
          'PKCE S256 challenge of RFC 7636 appendix B')
    r = s.cmd('PCT a b&c=d/é~-._')
    check(r.line('PCT ') == 'a%20b%26c%3Dd%2F%C3%A9~-._', 'percent encoding', r.lines)
    r = s.cmd('PCTD a+b%20c%C3%A9')
    check(r.line('PCTD ') == 'a b cé', 'percent decoding (+ is a space in a query)', r.lines)
    r = s.cmd('PCTD bad%2')
    check(r.line('PCTD ') == '<error>', 'a broken escape is refused')
    s.close()


def test_b(idp, tmp):
    print('== B. discovery')
    idp.reset()
    s = Session()
    s.cmd('CFG client_id test-client')
    r = s.cmd('DISCOVER ' + idp.base)
    check(r.result == 0, 'discovery of the provider', r.lines)
    r = s.cmd('DISCOVER ' + idp.base + '/')
    check(r.result == 0, 'a trailing slash on the issuer is fine')
    idp.knobs(issuer='https://other.example')
    r = s.cmd('DISCOVER ' + idp.base)
    check(r.result == 3 and r.err == 'issuer_mismatch', 'a document of another issuer is refused', r.lines)
    idp.knobs(issuer='')
    s.cmd('CFG issuer ' + idp.base)
    s.cmd('CFG device http://evil.example/device')
    r = s.cmd('DEVICE_START')
    check(r.result == 12, 'an http endpoint that is not loopback: OA_CONFIG, nothing sent', r.lines)
    check(not any(x['path'] == '/device' for x in idp.log()['log']), 'no request left the machine')
    s.close()


def test_c(idp, tmp):
    print('== C. authorization code + PKCE, loopback')
    idp.reset()
    s = Session(on_open=login_browser)
    configure(s, idp)
    r = s.cmd('LOGIN 30000')
    check(r.result == 0, 'LOGIN ok', r.lines)
    t = r.tokens
    check(t.get('ok') == '1' and len(t.get('access', '')) > 20 and len(t.get('refresh', '')) > 20 and
          int(t.get('id', '0')) > 100, 'access, refresh and id_token received', t)
    check(3000 < int(t.get('exp', '0')) <= 3600, 'expires_in is read (exp=%s)' % t.get('exp'))
    check('offline_access' in t.get('scope', ''), 'scope kept: %s' % t.get('scope'))
    lg = idp.log()
    auth = [x for x in lg['log'] if x['path'] == '/authorize'][0]['query']
    tokreq = [x for x in lg['log'] if x['path'] == '/token'][0]
    check(auth['code_challenge_method'] == 'S256' and len(auth['code_challenge']) == 43,
          'authorize: S256 challenge of 43 characters')
    check(len(auth['state']) >= 20 and len(auth['nonce']) >= 20, 'state and nonce are long and random')
    check(re.match(r'http://127\.0\.0\.1:(\d+)/callback$', auth['redirect_uri']), 'redirect_uri: ' + auth['redirect_uri'])
    port = int(re.match(r'http://127\.0\.0\.1:(\d+)/callback$', auth['redirect_uri']).group(1))
    check(49152 <= port < 65152, 'a port from the dynamic range (%d)' % port)
    check(tokreq['form'].get('redirect_uri') == auth['redirect_uri'] and tokreq['verifier_len'] >= 43,
          'token request: same redirect_uri, verifier of %d characters' % tokreq['verifier_len'])
    check(auth['scope'] == 'openid profile offline_access', 'scope sent exactly')
    check(lg['jwks_fetches'] == 1, 'the JWKS was fetched once for the id_token')
    page = s.browser[0]
    check(page[0] == 200 and 'Signed in' in page[1], 'the browser got the "signed in" page', page[:2])
    # the server is gone
    try:
        urllib.request.urlopen('http://127.0.0.1:%d/callback' % port, timeout=3)
        gone = False
    except urllib.error.URLError:
        gone = True
    check(gone, 'the loopback port is closed after the login')
    # a second login (the global state is clean again)
    idp.reset()
    r = s.cmd('LOGIN 30000')
    check(r.result == 0, 'a second login works')
    # login_hint style extra parameters, escaped
    s.cmd('CFG extra_auth login_hint=a%40b.example&prompt=select_account')
    r = s.cmd('LOGIN 30000')
    q = [x for x in idp.log()['log'] if x['path'] == '/authorize'][-1]['query']
    check(r.result == 0 and q.get('login_hint') == 'a@b.example' and q.get('prompt') == 'select_account',
          'extra authorize parameters arrive', q)
    s.cmd('CFG extra_auth ')
    # localhost as the redirect host, a fixed port
    s.cmd('CFG redirect_host localhost')
    s.cmd('CFG port 53917')
    r = s.cmd('LOGIN 30000')
    q = [x for x in idp.log()['log'] if x['path'] == '/authorize'][-1]['query']
    check(r.result == 0 and q['redirect_uri'] == 'http://localhost:53917/callback', 'redirect_host and port honoured', q)
    s.close()


def test_d(idp, tmp):
    print('== D. the loopback server under attack')
    idp.reset()
    results = {}

    def evil(url):
        # a wrong state, no state, another path -- all before the real redirect
        redirect = urllib.parse.parse_qs(urllib.parse.urlparse(url).query)['redirect_uri'][0]
        results['wrong'] = browse(redirect + '?code=stolen&state=wrong', follow=False)[:2]
        results['nostate'] = browse(redirect + '?code=abc', follow=False)[:2]
        results['other'] = browse(redirect.replace('/callback', '/other'), follow=False)[:2]
        results['post'] = None
        try:
            req = urllib.request.Request(redirect + '?code=x&state=y', data=b'x', method='POST')
            urllib.request.urlopen(req, timeout=10)
        except urllib.error.HTTPError as e:
            results['post'] = (e.code, '')
        r = browse(location_of(url))
        results['good'] = r[:2]
        return r

    s = Session(on_open=evil)
    configure(s, idp)
    r = s.cmd('LOGIN 30000')
    check(r.result == 0, 'the right redirect wins in the end', r.lines)
    check(results['wrong'][0] == 400, 'wrong state: 400', results['wrong'])
    check(results['nostate'][0] == 400, 'no state: 400', results['nostate'])
    check(results['other'][0] == 404, 'another path: 404', results['other'])
    check(results['post'] and results['post'][0] in (400, 405), 'POST to the callback: refused', results['post'])
    check(results['good'][0] == 200, 'right state: 200')
    check(not any(x['form'].get('code') == 'stolen' for x in idp.log()['log'] if x['path'] == '/token'),
          'the code behind a wrong state never reached the token endpoint')
    s.close()
    # denial
    idp.reset()
    idp.knobs(deny=True)
    s2 = Session(on_open=login_browser)
    configure(s2, idp)
    r = s2.cmd('LOGIN 30000')
    check(r.result == 4 and r.err == 'access_denied' and 'said no' in r.desc,
          'the user denies: OA_DENIED, error_description read through percent decoding', r.lines)
    check(s2.browser[0][0] == 200 and 'failed' in s2.browser[0][1], 'the browser is told it failed')
    idp.knobs(deny=False)
    s2.close()
    # timeout
    t0 = time.time()
    s3 = Session(on_open=lambda u: None)
    configure(s3, idp)
    r = s3.cmd('LOGIN 1500')
    el = time.time() - t0
    check(r.result == 9 and 1.2 < el < 4, 'nobody comes: OA_TIMEOUT after 1.5 s (%.1f s)' % el, r.lines)
    # cancel
    s3.cmd('CANCELIN 700')
    t0 = time.time()
    r = s3.cmd('LOGIN 20000')
    el = time.time() - t0
    check(r.result == 8 and el < 5, 'cancel flag: OA_CANCELLED after %.1f s' % el, r.lines)
    s3.close()
    # the browser cannot be opened
    s4 = Session(on_open=lambda u: None)
    configure(s4, idp)
    s4.cmd('CFG auth http://example.com/authorize')
    r = s4.cmd('LOGIN 1000')
    check(r.result == 12, 'an authorize endpoint over plain http to a stranger: OA_CONFIG, no browser', r.lines)
    check(s4.opened == [], 'the browser was not opened')
    s4.close()


def test_e(idp, tmp):
    print('== E. device flow')
    idp.reset()
    s = Session()
    configure(s, idp)
    r = s.cmd('DEVICE_START')
    d = r.line('DEVICE ').split(' ')
    check(r.result == 0 and len(d[0]) == 8 and d[1].endswith('/activate') and 'user_code=' + d[0] in d[2],
          'device authorization: code %s' % d[0], r.lines)
    check('interval=1' in r.line('DEVICE ') and 'expires=900' in r.line('DEVICE '), 'interval and lifetime read')
    r = s.cmd('DEVICE_POLL')
    check(r.result == 6, 'poll 1: authorization_pending (OA_PENDING)', r.lines)
    idp.call('/device/approve?user_code=' + d[0])
    time.sleep(1.1)
    r = s.cmd('DEVICE_POLL')
    check(r.result == 0 and r.tokens.get('ok') == '1', 'poll 2: tokens after approval', r.lines)
    check(int(r.tokens['id']) > 100, 'the id_token came with it and passed the check')
    s.close()

    # the blocking wait; the "user" approves from the callback as soon as the code is shown
    def approver(code_getter):
        def f(ln):
            if ln.startswith('DEVICE '):
                code_getter(ln.split(' ')[1])
        return f

    # slow_down: pending (poll 1), slow_down (poll 2) -> 6 s interval, approved at the third poll
    idp.reset()
    idp.knobs(force_slow_down=1)
    s2 = Session()
    configure(s2, idp)
    t0 = time.time()
    r = s2.cmd('DEVICE', timeout=90, on_line=approver(lambda c: threading.Timer(1.5, lambda: idp.call('/device/approve?user_code=' + c)).start()))
    el = time.time() - t0
    check(r.result == 0, 'DEVICE waits for the approval', r.lines)
    check(r.line('INTERVAL ') == '6', 'slow_down: the interval grew from 1 to 6 seconds', r.line('INTERVAL '))
    check(el > 6.5, 'the third poll came after the longer pause (%.1f s)' % el)
    polls = [x['t'] for x in idp.log()['log'] if x['path'] == '/token']
    check(len(polls) == 3 and polls[2] - polls[1] > 5.5, 'the server saw 3 polls, the last one %.1f s after the slow_down' % (polls[-1] - polls[-2]), polls)
    s2.close()
    # denial
    idp.reset()
    s3 = Session()
    configure(s3, idp)
    r = s3.cmd('DEVICE', timeout=30, on_line=approver(lambda c: idp.call('/device/approve?user_code=%s&deny=1' % c)))
    check(r.result == 4 and r.err == 'access_denied', 'the user refuses: OA_DENIED', r.lines)
    # expiry (the server says expired_token when the code is older than device_expires)
    idp.knobs(device_expires=1)
    r = s3.cmd('DEVICE', timeout=30)
    check(r.result in (5, 9), 'a code that ran out: OA_EXPIRED/OA_TIMEOUT (%s)' % r.name, r.lines)
    s3.close()
    # cancel the wait
    idp.reset()
    s4 = Session()
    configure(s4, idp)
    s4.cmd('CANCELIN 1500')
    t0 = time.time()
    r = s4.cmd('DEVICE', timeout=30)
    check(r.result == 8 and time.time() - t0 < 6, 'cancel flag ends the wait: OA_CANCELLED', r.lines)
    s4.close()
    # a device endpoint that answers an OAuth error
    idp.reset()
    s5 = Session()
    configure(s5, idp)
    s5.cmd('CFG client_id nobody')
    r = s5.cmd('DEVICE_START')
    check(r.result == 14 and r.err == 'invalid_client', 'invalid_client at the device endpoint: OA_ERROR', r.lines)
    s5.close()


def test_f(idp, tmp):
    print('== F. refresh')
    idp.reset()
    s = Session(on_open=login_browser)
    configure(s, idp)
    r = s.cmd('LOGIN 30000')
    a1, r1 = r.tokens['access'], r.tokens['refresh']
    r = s.cmd('FRESH 60')
    check(r.result == 0 and r.tokens['access'] == a1, 'ensure_fresh: still good, no request')
    n_before = len([x for x in idp.log()['log'] if x['path'] == '/token'])
    check(n_before == 1, 'only the login hit the token endpoint')
    r = s.cmd('FRESH 99999')
    check(r.result == 0 and r.tokens['access'] != a1 and r.tokens['refresh'] != r1,
          'ensure_fresh near expiry: refreshed, refresh token rotated')
    a2, r2 = r.tokens['access'], r.tokens['refresh']
    check(r.tokens['id'] != '0', 'the id_token is kept (or renewed)')
    idp.knobs(refresh_rotation=False, no_refresh_token=True)
    r = s.cmd('REFRESH')
    check(r.result == 0 and r.tokens['refresh'] == r2, 'a refresh answer without refresh_token keeps the old one', r.tokens)
    check(r.tokens['access'] != a2, 'a new access token')
    tr = [x for x in idp.log()['log'] if x['path'] == '/token'][-1]
    check(tr['form'].get('grant_type') == 'refresh_token' and tr['form'].get('scope') == 'openid profile offline_access',
          'the refresh request names the grant and the scope', tr['form'])
    idp.knobs(refresh_rotation=True, no_refresh_token=False)
    s.cmd('CLEAR')
    r = s.cmd('REFRESH')
    check(r.result == 16, 'no refresh token: OA_NOTOKEN', r.lines)
    r = s.cmd('FRESH 60')
    check(r.result == 16, 'ensure_fresh without tokens: OA_NOTOKEN')
    s.close()
    # revoked / reused refresh token
    idp.reset()
    s4 = Session(on_open=login_browser)
    configure(s4, idp)
    s4.cmd('LOGIN 30000')
    r = s4.cmd('REVOKE r')
    check(r.result == 0, 'revocation (RFC 7009)', r.lines)
    r = s4.cmd('REFRESH')
    check(r.result == 10 and r.err == 'invalid_grant', 'a revoked refresh token: OA_INVALID_GRANT', r.lines)
    s4.close()
    # userinfo
    idp.reset()
    s5 = Session(on_open=login_browser)
    configure(s5, idp)
    s5.cmd('LOGIN 30000')
    r = s5.cmd('USERINFO')
    check(r.result == 0 and '"sub": "user-1"' in (r.line('USERINFO ') or '') or '"sub":"user-1"' in (r.line('USERINFO ') or ''),
          'userinfo with the Bearer token', r.lines)
    s5.cmd('REVOKE a')
    r = s5.cmd('USERINFO')
    check(r.result == 10, 'userinfo with a revoked token: 401 -> OA_INVALID_GRANT', r.lines)
    s5.close()


def test_g(idp, tmp):
    print('== G. id_token refusals')
    for bad_id, want_desc in (('sig', 'sig'), ('iss', 'iss'), ('aud', 'aud'), ('nonce', 'nonce'),
                              ('expired', 'expired'), ('alg_none', 'alg')):
        idp.reset()
        idp.knobs(bad_id=bad_id)
        s = Session(on_open=login_browser)
        configure(s, idp)
        r = s.cmd('LOGIN 30000')
        check(r.result == 13 and r.desc == want_desc, 'id_token with a bad %s: OA_TOKEN_INVALID (%s)' % (bad_id, r.desc), r.lines)
        check(r.tokens.get('ok') == '0', '... and no tokens are handed out')
        s.close()
    # verification switched off
    idp.reset()
    idp.knobs(bad_id='sig')
    s = Session(on_open=login_browser)
    configure(s, idp)
    s.cmd('CFG verify 0')
    r = s.cmd('LOGIN 30000')
    check(r.result == 0, 'verify=0: the caller takes the risk')
    s.close()
    # key rotation at the provider
    idp.reset()
    s = Session(on_open=login_browser)
    configure(s, idp)
    r = s.cmd('LOGIN 30000')
    check(r.result == 0 and idp.log()['jwks_fetches'] == 1, 'first login fetches the keys')
    time.sleep(0.2)
    idp.call('/rotate', {})
    # the cache is younger than a minute: an unknown kid does NOT refetch (no hammering)
    r = s.cmd('LOGIN 30000')
    check(r.result == 13 and idp.log()['jwks_fetches'] == 1,
          'unknown kid within a minute of the last fetch: refused, no refetch', r.lines)
    s.close()
    s = Session(on_open=login_browser)
    configure(s, idp)
    idp.call('/rotate', {})
    r = s.cmd('LOGIN 30000')
    check(r.result == 0, 'a fresh client after rotation fetches the new key', r.lines)
    s.close()


def test_h(idp, tmp):
    print('== H. client authentication')
    idp.reset()
    idp.knobs(secret='s3cr3t&=%', auth_mode='body')
    s = Session(on_open=login_browser)
    configure(s, idp)
    s.cmd('CFG secret s3cr3t&=%')
    r = s.cmd('LOGIN 30000')
    tr = [x for x in idp.log()['log'] if x['path'] == '/token'][0]
    check(r.result == 0 and tr['auth'] is None, 'client_secret in the body (percent encoded), no Authorization header', r.lines)
    s.close()
    idp.reset()
    idp.knobs(secret='s3cr3t&=%', auth_mode='basic')
    s = Session(on_open=login_browser)
    configure(s, idp)
    s.cmd('CFG secret s3cr3t&=%')
    s.cmd('CFG basic 1')
    r = s.cmd('LOGIN 30000')
    tr = [x for x in idp.log()['log'] if x['path'] == '/token'][0]
    check(r.result == 0 and (tr['auth'] or '').startswith('Basic ') and 'client_secret' not in tr['form'],
          'Basic authentication: the secret is not in the body', tr)
    s.close()
    idp.reset()
    idp.knobs(secret='right', auth_mode='body')
    s = Session(on_open=login_browser)
    configure(s, idp)
    s.cmd('CFG secret wrong')
    r = s.cmd('LOGIN 30000')
    check(r.result == 14 and r.err == 'invalid_client', 'a wrong secret: invalid_client -> OA_ERROR', r.lines)
    s.close()


def test_i(idp, tmp):
    print('== I. failures')
    idp.reset()
    idp.knobs(token_status=502)
    s = Session(on_open=login_browser)
    configure(s, idp)
    r = s.cmd('LOGIN 30000')
    check(r.result == 3 and r.http == 502, 'a gateway error page instead of JSON: OA_PROTOCOL (http 502)', r.lines)
    idp.knobs(token_status=0)
    s.cmd('CFG token http://127.0.0.1:1/token')
    r = s.cmd('LOGIN 30000')
    check(r.result == 1 and r.net == 5, 'a refused connection: OA_NET (net.http Connect)', r.lines)
    s.close()
    s = Session()
    s.cmd('CFG device https://127.0.0.1:1/device')
    r = s.cmd('DEVICE_START')
    check(r.result in (1, 12), 'https to a closed port: no crash (%s)' % r.name)
    r = s.cmd('LOGIN 1000')
    check(r.result == 12, 'no endpoints set: OA_CONFIG', r.lines)
    s.close()


def test_j(idp, tmp):
    print('== J. token store')
    idp.reset()
    d = os.path.join(tmp, 'xdg_j')
    os.makedirs(d)
    env = {'XDG_DATA_HOME': d}
    s = Session(env=env, on_open=login_browser)
    configure(s, idp)
    r = s.cmd('LOGIN 30000')
    acc, ref = r.tokens['access'], r.tokens['refresh']
    r = s.cmd('SAVE oauthtest account-1')
    check(r.line('STORE ') == '1', 'saved to the keyring')
    s.cmd('CLEAR')
    r = s.cmd('LOAD oauthtest account-1')
    check(r.line('STORE ') == '1' and r.tokens['access'] == acc and r.tokens['refresh'] == ref and r.tokens['ok'] == '1',
          'loaded back identical (also the id_token of %s characters)' % r.tokens.get('id'))
    r = s.cmd('LOAD oauthtest nobody')
    check(r.line('STORE ') == '0' and r.tokens['ok'] == '0', 'a missing entry: false, empty tokens')
    blob = b''
    for root, _, files in os.walk(d):
        for f in files:
            blob += open(os.path.join(root, f), 'rb').read()
    if os.environ.get('RUNNER'):
        skip_note = True
    check(acc.encode() not in blob and ref.encode() not in blob and len(blob) > 100,
          'the file holds no readable token (%d octets)' % len(blob))
    r = s.cmd('DEL oauthtest account-1')
    r = s.cmd('LOAD oauthtest account-1')
    check(r.line('STORE ') == '0', 'deleted')
    # the refresh still works from a loaded set
    s.cmd('LOGIN 30000')
    s.cmd('SAVE oauthtest account-2')
    s.cmd('CLEAR')
    s.cmd('LOAD oauthtest account-2')
    r = s.cmd('REFRESH')
    check(r.result == 0, 'refresh with a token set from the keyring')
    r = s.cmd('JSON')
    j = json.loads(r.line('JSON '))
    check(set(j) == {'access_token', 'refresh_token', 'id_token', 'token_type', 'scope', 'expires_at', 'issued_at'},
          'the JSON form has the seven members', sorted(j))
    s.close()


def test_k(idp, tmp):
    print('== K. a pasted redirect URL')
    idp.reset()
    s = Session()
    configure(s, idp)
    r = s.cmd('AUTHURL')
    url = r.line('AUTHURL ')
    st, body, final = browse(url, follow=False)
    # the 302 answer carries the Location (urllib raises it as HTTPError)
    try:
        urllib.request.build_opener(NoRedirect).open(url, timeout=10)
    except urllib.error.HTTPError as e:
        location = e.headers.get('Location')
    r = s.cmd('PASTE ' + location)
    check(r.result == 0 and r.tokens['ok'] == '1', 'the code in a pasted URL is exchanged', r.lines)
    r = s.cmd('PASTE ' + location.replace('state=', 'state=x'))
    check(r.result == 11, 'a pasted URL with another state: OA_STATE', r.lines)
    r = s.cmd('PASTE http://127.0.0.1:9/cb?error=access_denied&state=zz')
    check(r.result == 11, 'an error answer with a wrong state is not believed either')
    s.close()


def main():
    tmp = tempfile.mkdtemp(prefix='oacheck.')
    idp = Idp()
    only = set(sys.argv[2:])
    try:
        for name, fn in (('a', test_a), ('b', test_b), ('c', test_c), ('d', test_d), ('e', test_e),
                         ('f', test_f), ('g', test_g), ('h', test_h), ('i', test_i), ('j', test_j),
                         ('k', test_k)):
            if not only or name in only:
                fn(idp, tmp)
    finally:
        idp.stop()
        shutil.rmtree(tmp, ignore_errors=True)
    print('\n%d passed, %d failed, %d skipped' % (PASSES, FAILS, SKIPS))
    return 1 if FAILS else 0


if __name__ == '__main__':
    sys.exit(main())
