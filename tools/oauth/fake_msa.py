#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/oauth/fake_msa.py -- the Microsoft -> Xbox Live -> XSTS -> Minecraft
services chain, written from the published protocol and strict about it
(every header, every field of every body), for tools/oauth/check_msa.py.
Plain http on 127.0.0.1; the paths stand in for the five hosts:

  POST /devicecode               Microsoft device authorization
  POST /token                    device_code and refresh_token grants
  POST /xbl/user/authenticate    Xbox Live user token
  POST /xsts/authorize           XSTS token (XErr answers by knob)
  POST /mc/authentication/login_with_xbox
  GET  /mc/entitlements/mcstore  GET /mc/minecraft/profile
  POST /knobs  GET /log  POST /reset  GET /approve

Prints "port <n>".
"""
import base64
import http.server
import json
import secrets
import socketserver
import sys
import threading
import time
import urllib.parse

LOCK = threading.Lock()
S = {}


def reset():
    S.clear()
    S.update({
        'log': [], 'ms_access': {}, 'ms_refresh': {}, 'xbl': {}, 'xsts': {}, 'mc': {},
        'device': None, 'approved': False, 'n': 0,
        'knobs': {'xerr': 0, 'mc_refuse': False, 'owns': True, 'has_profile': True,
                  'xbl_status': 0, 'pending': 0, 'name': 'Steve_42', 'ms_expires': 3600,
                  'mc_expires': 86400, 'bad_xbl_shape': False, 'junk_xsts': False}})


reset()
CLIENT = 'azure-client-id'
UUID = '069a79f444e94726a5befca90e38aaf5'


def b64(b):
    return base64.urlsafe_b64encode(b).rstrip(b'=').decode()


def jwt_like(claims):
    return b64(b'{"alg":"RS256"}') + '.' + b64(json.dumps(claims).encode()) + '.' + b64(b'sig')


class H(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def log_message(self, *a):
        pass

    def reply(self, code, obj=None, raw=None, ctype='application/json'):
        body = raw if raw is not None else json.dumps(obj).encode()
        self.send_response(code)
        self.send_header('Content-Type', ctype)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def read(self):
        n = int(self.headers.get('Content-Length') or 0)
        return self.rfile.read(n) if n else b''

    def note(self, **kw):
        rec = {'path': self.path, 'method': self.command, 'ctype': self.headers.get('Content-Type'),
               'accept': self.headers.get('Accept'), 'auth': self.headers.get('Authorization'), 't': time.time()}
        rec.update(kw)
        with LOCK:
            S['log'].append(rec)

    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        k = S['knobs']
        if u.path == '/log':
            return self.reply(200, S['log'])
        if u.path == '/approve':
            S['approved'] = True
            return self.reply(200, {'ok': True})
        if u.path.startswith('/mc/'):
            tok = (self.headers.get('Authorization') or '')[7:]
            self.note(token_known=tok in S['mc'])
            if tok not in S['mc'] or not (self.headers.get('Authorization') or '').startswith('Bearer '):
                return self.reply(401, {'path': u.path, 'errorType': 'UNAUTHORIZED', 'error': 'UNAUTHORIZED'})
            if u.path == '/mc/entitlements/mcstore':
                items = []
                if k['owns']:
                    items = [{'name': 'product_minecraft', 'signature': 'x'}, {'name': 'game_minecraft', 'signature': 'y'}]
                return self.reply(200, {'items': items, 'signature': 'z', 'keyId': '1'})
            if u.path == '/mc/minecraft/profile':
                if not k['has_profile']:
                    return self.reply(404, {'path': '/minecraft/profile', 'errorType': 'NOT_FOUND', 'error': 'NOT_FOUND',
                                            'errorMessage': 'The server has not found anything matching the request URI'})
                return self.reply(200, {'id': UUID, 'name': k['name'], 'skins': [{'id': 's', 'state': 'ACTIVE'}], 'capes': []})
        return self.reply(404, {'error': 'unknown'})

    def do_POST(self):
        u = urllib.parse.urlparse(self.path)
        raw = self.read()
        k = S['knobs']
        if u.path == '/knobs':
            k.update(json.loads(raw))
            return self.reply(200, {'ok': True})
        if u.path == '/reset':
            reset()
            return self.reply(200, {'ok': True})
        if u.path == '/devicecode':
            f = {a: b[0] for a, b in urllib.parse.parse_qs(raw.decode()).items()}
            self.note(form=f)
            if f.get('client_id') != CLIENT:
                return self.reply(400, {'error': 'unauthorized_client', 'error_description': 'AADSTS700016 unknown'})
            dc = secrets.token_urlsafe(20)
            S['device'] = {'code': dc, 'polls': 0}
            return self.reply(200, {'device_code': dc, 'user_code': 'ABCD1234', 'verification_uri': 'https://microsoft.com/link',
                                    'expires_in': 900, 'interval': 1, 'message': 'go'})
        if u.path == '/token':
            f = {a: b[0] for a, b in urllib.parse.parse_qs(raw.decode()).items()}
            self.note(form=f)
            if f.get('grant_type') == 'urn:ietf:params:oauth:grant-type:device_code':
                d = S['device']
                if not d or f.get('device_code') != d['code']:
                    return self.reply(400, {'error': 'invalid_grant'})
                d['polls'] += 1
                if d['polls'] <= k['pending'] or not S['approved']:
                    return self.reply(400, {'error': 'authorization_pending'})
                return self.ms_tokens(f)
            if f.get('grant_type') == 'refresh_token':
                r = S['ms_refresh'].pop(f.get('refresh_token', ''), None)
                if not r:
                    return self.reply(400, {'error': 'invalid_grant', 'error_description': 'AADSTS70000'})
                return self.ms_tokens(f)
            return self.reply(400, {'error': 'unsupported_grant_type'})
        if u.path == '/xbl/user/authenticate':
            try:
                b = json.loads(raw)
            except Exception:
                b = None
            self.note(body=b)
            if k['xbl_status']:
                return self.reply(k['xbl_status'], {'Identity': '0', 'XErr': 2148916233, 'Message': ''})
            if self.headers.get('Content-Type', '').split(';')[0] != 'application/json':
                return self.reply(400, {'error': 'content-type'})
            p = (b or {}).get('Properties', {})
            ok = (b is not None and p.get('AuthMethod') == 'RPS' and p.get('SiteName') == 'user.auth.xboxlive.com'
                  and p.get('RpsTicket', '').startswith('d=') and p['RpsTicket'][2:] in S['ms_access']
                  and b.get('RelyingParty') == 'http://auth.xboxlive.com' and b.get('TokenType') == 'JWT'
                  and set(b) == {'Properties', 'RelyingParty', 'TokenType'})
            if not ok:
                return self.reply(400, {'Identity': '0', 'XErr': 2148916238, 'Message': 'bad shape'})
            S['n'] += 1
            tok = 'XBL-%d' % S['n']
            S['xbl'][tok] = True
            if k['bad_xbl_shape']:
                return self.reply(200, {'Token': tok})
            return self.reply(200, {'IssueInstant': '2026-01-01T00:00:00.0Z', 'NotAfter': '2026-01-15T00:00:00.0Z',
                                    'Token': tok, 'DisplayClaims': {'xui': [{'uhs': 'UHS-' + tok}]}})
        if u.path == '/xsts/authorize':
            try:
                b = json.loads(raw)
            except Exception:
                b = None
            self.note(body=b)
            p = (b or {}).get('Properties', {})
            ok = (b is not None and p.get('SandboxId') == 'RETAIL' and isinstance(p.get('UserTokens'), list)
                  and len(p['UserTokens']) == 1 and p['UserTokens'][0] in S['xbl']
                  and b.get('RelyingParty') == 'rp://api.minecraftservices.com/' and b.get('TokenType') == 'JWT')
            if not ok:
                return self.reply(400, {'Identity': '0', 'XErr': 2148916238, 'Message': 'bad shape'})
            if k['xerr']:
                return self.reply(401, {'Identity': '0', 'XErr': k['xerr'], 'Message': '',
                                        'Redirect': 'https://start.ui.xboxlive.com/CreateAccount'})
            S['n'] += 1
            tok = 'XSTS-%d' % S['n']
            S['xsts'][tok] = 'UHS-' + p['UserTokens'][0]
            return self.reply(200, {'IssueInstant': 'x', 'NotAfter': 'y', 'Token': tok,
                                    'DisplayClaims': {'xui': [{'uhs': S['xsts'][tok]}]}})
        if u.path == '/mc/authentication/login_with_xbox':
            try:
                b = json.loads(raw)
            except Exception:
                b = None
            self.note(body=b)
            idt = (b or {}).get('identityToken', '')
            m = idt.startswith('XBL3.0 x=') and ';' in idt
            uhs, _, xsts = idt[9:].partition(';')
            if not m or xsts not in S['xsts'] or S['xsts'][xsts] != uhs or set(b) != {'identityToken'} or k['mc_refuse']:
                return self.reply(401, {'path': '/authentication/login_with_xbox', 'errorType': 'UNAUTHORIZED',
                                        'error': 'UNAUTHORIZED', 'errorMessage': 'Invalid app registration'})
            S['n'] += 1
            tok = 'MC-' + jwt_like({'xuid': '2535', 'agg': 'Adult', 'sub': UUID, 'n': S['n']})
            S['mc'][tok] = True
            return self.reply(200, {'username': UUID, 'roles': [], 'access_token': tok, 'token_type': 'Bearer',
                                    'expires_in': k['mc_expires']})
        return self.reply(404, {'error': 'unknown'})

    def ms_tokens(self, f):
        k = S['knobs']
        S['n'] += 1
        at = 'MSAT-%d' % S['n']
        rt = 'MSRT-%d' % S['n']
        S['ms_access'][at] = True
        S['ms_refresh'][rt] = True
        return self.reply(200, {'token_type': 'Bearer', 'scope': 'XboxLive.signin offline_access',
                                'expires_in': k['ms_expires'], 'access_token': at, 'refresh_token': rt})


class Srv(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True
    allow_reuse_address = True

    def handle_error(self, request, client_address):
        pass


if __name__ == '__main__':
    srv = Srv(('127.0.0.1', 0), H)
    print('port %d' % srv.server_address[1], flush=True)
    srv.serve_forever()
