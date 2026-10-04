#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/oauth/fake_idp.py -- an OpenID Connect identity provider that is
strict where it should be and misbehaves where told to, for
tools/oauth/check.py. Plain http on 127.0.0.1 (the client accepts http for
loopback hosts only). Prints "port <n>" on stdout.

  GET  /.well-known/openid-configuration   discovery (issuer = this base URL)
  GET  /jwks                               the public keys (RSA, kid k<n>)
  POST /rotate                             a new signing key, the old one vanishes
  GET  /authorize                          validates the request, answers 302 to the
                                           redirect_uri with code+state (or an error)
  POST /device                             device authorization (interval 1 s)
  GET  /device/approve?user_code=..        "the user" approves (&deny=1 refuses)
  POST /token                              authorization_code (PKCE S256 checked),
                                           refresh_token (rotating), device_code
  GET  /userinfo                           Bearer access token -> {"sub":..}
  POST /revoke
  POST /knobs                              JSON: expires_in, bad_id, deny, token_status,
                                           device_expires, force_slow_down, ...
  GET  /log   POST /reset                  what the server saw

bad_id: "" | sig | iss | aud | nonce | expired | alg_none
"""
import base64
import hashlib
import http.server
import json
import secrets
import socketserver
import sys
import threading
import time
import urllib.parse

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric import padding, rsa

LOCK = threading.Lock()


def b64(b):
    return base64.urlsafe_b64encode(b).rstrip(b'=').decode()


def i2b(n):
    return n.to_bytes((n.bit_length() + 7) // 8, 'big')


class Keys:
    def __init__(self):
        self.n = 0
        self.rotate()

    def rotate(self):
        self.n += 1
        self.kid = 'k%d' % self.n
        self.key = rsa.generate_private_key(public_exponent=65537, key_size=2048)

    def jwks(self):
        pn = self.key.public_key().public_numbers()
        return {'keys': [{'kty': 'RSA', 'kid': self.kid, 'use': 'sig', 'alg': 'RS256',
                          'n': b64(i2b(pn.n)), 'e': b64(i2b(pn.e))}]}

    def sign(self, data):
        return self.key.sign(data, padding.PKCS1v15(), hashes.SHA256())


KEYS = Keys()
STATE = {}


def reset_state():
    STATE.clear()
    STATE.update({
        'base': '', 'codes': {}, 'refresh': {}, 'access': {}, 'devices': {}, 'log': [],
        'knobs': {'expires_in': 3600, 'bad_id': '', 'deny': False, 'token_status': 0,
                  'force_slow_down': 0, 'device_expires': 900, 'refresh_rotation': True,
                  'no_refresh_token': False, 'omit_id_token': False, 'issuer': '',
                  'secret': '', 'auth_mode': ''},
        'jwks_fetches': 0,
    })


reset_state()


def id_token(client_id, sub, nonce, knobs, base):
    now = int(time.time())
    claims = {'iss': knobs['issuer'] or base, 'aud': client_id, 'sub': sub, 'iat': now,
              'exp': now + 3600, 'name': 'Test User'}
    if nonce:
        claims['nonce'] = nonce
    bad = knobs['bad_id']
    if bad == 'iss':
        claims['iss'] = 'https://evil.example'
    elif bad == 'aud':
        claims['aud'] = 'someone-else'
    elif bad == 'nonce' and nonce:
        claims['nonce'] = 'wrong'
    elif bad == 'expired':
        claims['exp'] = now - 3600
    alg = 'none' if bad == 'alg_none' else 'RS256'
    head = {'alg': alg, 'typ': 'JWT', 'kid': KEYS.kid}
    si = b64(json.dumps(head).encode()) + '.' + b64(json.dumps(claims).encode())
    sig = b'' if alg == 'none' else KEYS.sign(si.encode())
    if bad == 'sig':
        sig = bytes([sig[0] ^ 1]) + sig[1:]
    return si + '.' + b64(sig)


class H(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def log_message(self, *a):
        pass

    def reply(self, code, obj=None, raw=None, ctype='application/json', headers=None):
        body = raw if raw is not None else json.dumps(obj).encode()
        self.send_response(code)
        self.send_header('Content-Type', ctype)
        self.send_header('Content-Length', str(len(body)))
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(body)

    def form(self):
        n = int(self.headers.get('Content-Length') or 0)
        raw = self.rfile.read(n) if n else b''
        return raw, {k: v[0] for k, v in urllib.parse.parse_qs(raw.decode(), keep_blank_values=True).items()}

    def note(self, **kw):
        rec = {'path': self.path.split('?')[0], 'method': self.command,
               'auth': self.headers.get('Authorization'), 't': time.time()}
        rec.update(kw)
        with LOCK:
            STATE['log'].append(rec)

    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        q = {k: v[0] for k, v in urllib.parse.parse_qs(u.query).items()}
        base = STATE['base']
        k = STATE['knobs']
        if u.path == '/.well-known/openid-configuration':
            self.note()
            return self.reply(200, {
                'issuer': k['issuer'] or base, 'authorization_endpoint': base + '/authorize',
                'token_endpoint': base + '/token', 'device_authorization_endpoint': base + '/device',
                'jwks_uri': base + '/jwks', 'userinfo_endpoint': base + '/userinfo',
                'revocation_endpoint': base + '/revoke',
                'response_types_supported': ['code'], 'code_challenge_methods_supported': ['S256']})
        if u.path == '/jwks':
            with LOCK:
                STATE['jwks_fetches'] += 1
            self.note()
            return self.reply(200, KEYS.jwks())
        if u.path == '/authorize':
            return self.authorize(q)
        if u.path == '/device/approve':
            d = STATE['devices'].get(q.get('user_code', ''))
            if not d:
                return self.reply(404, {'error': 'no such code'})
            d['approved'] = not q.get('deny')
            d['denied'] = bool(q.get('deny'))
            return self.reply(200, {'ok': True})
        if u.path == '/userinfo':
            self.note()
            tok = (self.headers.get('Authorization') or '')[7:]
            a = STATE['access'].get(tok)
            if not a:
                return self.reply(401, {'error': 'invalid_token'})
            return self.reply(200, {'sub': a['sub'], 'name': 'Test User'})
        if u.path == '/log':
            with LOCK:
                return self.reply(200, {'log': STATE['log'], 'jwks_fetches': STATE['jwks_fetches']})
        return self.reply(404, {'error': 'unknown ' + u.path})

    def authorize(self, q):
        k = STATE['knobs']
        self.note(query=q)
        err = None
        if q.get('response_type') != 'code':
            err = 'unsupported_response_type'
        elif q.get('code_challenge_method') != 'S256' or not q.get('code_challenge'):
            err = 'invalid_request'
        elif not q.get('redirect_uri', '').startswith('http://127.0.0.1:') and \
                not q.get('redirect_uri', '').startswith('http://localhost:'):
            err = 'invalid_request'
        redirect = q.get('redirect_uri', '')
        sep = '&' if '?' in redirect else '?'
        if err:
            return self.reply(400, {'error': err})
        if k['deny']:
            loc = redirect + sep + urllib.parse.urlencode({'error': 'access_denied',
                                                             'error_description': 'The user said no (&=)',
                                                             'state': q.get('state', '')})
            return self.reply(302, raw=b'', headers={'Location': loc})
        code = secrets.token_urlsafe(24)
        with LOCK:
            STATE['codes'][code] = {'challenge': q['code_challenge'], 'redirect': redirect,
                                    'client': q.get('client_id'), 'nonce': q.get('nonce', ''),
                                    'scope': q.get('scope', ''), 'used': False}
        loc = redirect + sep + urllib.parse.urlencode({'code': code, 'state': q.get('state', '')})
        return self.reply(302, raw=b'', headers={'Location': loc})

    def do_POST(self):
        u = urllib.parse.urlparse(self.path)
        raw, f = self.form()
        k = STATE['knobs']
        base = STATE['base']
        if u.path == '/knobs':
            with LOCK:
                k.update(json.loads(raw))
            return self.reply(200, {'ok': True})
        if u.path == '/reset':
            reset_state()
            STATE['base'] = base
            return self.reply(200, {'ok': True})
        if u.path == '/rotate':
            KEYS.rotate()
            return self.reply(200, {'kid': KEYS.kid})
        if u.path == '/device':
            self.note(form=f)
            if f.get('client_id') != 'test-client':
                return self.reply(400, {'error': 'invalid_client'})
            dc = secrets.token_urlsafe(24)
            uc = ''.join(secrets.choice('ABCDEFGHJKMNPQRSTUVWXYZ23456789') for _ in range(8))
            with LOCK:
                STATE['devices'][uc] = {'device_code': dc, 'approved': False, 'denied': False,
                                        'polls': [], 'slowed': 0, 'born': time.time(),
                                        'client': f['client_id']}
                STATE['devices'][dc] = STATE['devices'][uc]
            return self.reply(200, {'device_code': dc, 'user_code': uc,
                                    'verification_uri': base + '/activate',
                                    'verification_uri_complete': base + '/activate?user_code=' + uc,
                                    'expires_in': k['device_expires'], 'interval': 1})
        if u.path == '/token':
            return self.token(raw, f)
        if u.path == '/revoke':
            self.note(form=f)
            for store in ('refresh', 'access'):
                STATE[store].pop(f.get('token', ''), None)
            return self.reply(200, raw=b'')
        return self.reply(404, {'error': 'unknown ' + u.path})

    def check_client(self, f):
        k = STATE['knobs']
        hdr = self.headers.get('Authorization')
        cid = f.get('client_id')
        if k['secret']:
            if k['auth_mode'] == 'basic':
                # RFC 6749 2.3.1: id and secret are form-encoded before Basic
                if not hdr or not hdr.startswith('Basic '):
                    return False
                try:
                    user, _, pw = base64.b64decode(hdr[6:]).decode().partition(':')
                except Exception:
                    return False
                return urllib.parse.unquote(user) == 'test-client' and urllib.parse.unquote(pw) == k['secret']
            return f.get('client_secret') == k['secret'] and hdr is None
        return cid == 'test-client'

    def token(self, raw, f):
        k = STATE['knobs']
        base = STATE['base']
        self.note(form={a: ('<%d chars>' % len(b) if a in ('code_verifier',) else b) for a, b in f.items()},
                  verifier_len=len(f.get('code_verifier', '')),
                  verifier_ok=None)
        if k['token_status']:
            return self.reply(k['token_status'], raw=b'<html>Bad gateway</html>', ctype='text/html')
        if not self.check_client(f):
            return self.reply(401, {'error': 'invalid_client'})
        gt = f.get('grant_type')
        if gt == 'authorization_code':
            c = STATE['codes'].get(f.get('code', ''))
            if not c or c['used']:
                return self.reply(400, {'error': 'invalid_grant', 'error_description': 'code unknown or used'})
            c['used'] = True
            if c['redirect'] != f.get('redirect_uri'):
                return self.reply(400, {'error': 'invalid_grant', 'error_description': 'redirect_uri differs'})
            v = f.get('code_verifier', '')
            chal = b64(hashlib.sha256(v.encode()).digest())
            if len(v) < 43 or len(v) > 128 or chal != c['challenge']:
                return self.reply(400, {'error': 'invalid_grant', 'error_description': 'PKCE'})
            return self.issue(f['client_id'] if 'client_id' in f else 'test-client', 'user-1', c['nonce'], c['scope'])
        if gt == 'refresh_token':
            r = STATE['refresh'].get(f.get('refresh_token', ''))
            if not r:
                return self.reply(400, {'error': 'invalid_grant', 'error_description': 'refresh token unknown'})
            if k['refresh_rotation']:
                STATE['refresh'].pop(f['refresh_token'])
            return self.issue('test-client', r['sub'], '', r['scope'], reuse_refresh=None if k['refresh_rotation'] else f['refresh_token'])
        if gt == 'urn:ietf:params:oauth:grant-type:device_code':
            d = STATE['devices'].get(f.get('device_code', ''))
            if not d:
                return self.reply(400, {'error': 'invalid_grant'})
            now = time.time()
            d['polls'].append(now)
            if now - d['born'] > k['device_expires']:
                return self.reply(400, {'error': 'expired_token'})
            if d['denied']:
                return self.reply(400, {'error': 'access_denied'})
            # strict: a poll sooner than the interval is slow_down
            interval = 1 + 5 * d['slowed']
            if len(d['polls']) > 1 and now - d['polls'][-2] < interval - 0.15:
                d['slowed'] += 1
                return self.reply(400, {'error': 'slow_down'})
            if k['force_slow_down'] > 0 and len(d['polls']) == 2:
                k['force_slow_down'] -= 1
                d['slowed'] += 1
                return self.reply(400, {'error': 'slow_down'})
            if not d['approved']:
                return self.reply(400, {'error': 'authorization_pending'})
            return self.issue('test-client', 'user-1', '', 'openid')
        return self.reply(400, {'error': 'unsupported_grant_type'})

    def issue(self, client, sub, nonce, scope, reuse_refresh=None):
        k = STATE['knobs']
        base = STATE['base']
        at = secrets.token_urlsafe(24)
        STATE['access'][at] = {'sub': sub}
        out = {'access_token': at, 'token_type': 'Bearer', 'expires_in': k['expires_in'], 'scope': scope}
        if not k['no_refresh_token']:
            rt = reuse_refresh or secrets.token_urlsafe(24)
            STATE['refresh'][rt] = {'sub': sub, 'scope': scope}
            out['refresh_token'] = rt
        if 'openid' in scope.split() and not k['omit_id_token']:
            out['id_token'] = id_token(client, sub, nonce, k, base)
        return self.reply(200, out, headers={'Cache-Control': 'no-store'})


class S(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True

    def handle_error(self, request, client_address):
        pass


if __name__ == '__main__':
    srv = S(('127.0.0.1', 0), H)
    STATE['base'] = 'http://127.0.0.1:%d' % srv.server_address[1]
    print('port %d' % srv.server_address[1], flush=True)
    srv.serve_forever()
