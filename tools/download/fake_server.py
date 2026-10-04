#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/download/fake_server.py -- an HTTP/1.1 server that misbehaves on
purpose, for tools/download/check.py.

Prints "port <n>" on stdout and serves until killed. Every content is
DETERMINISTIC: content(name, size) is a pseudo random byte string derived
from the name, so the checker can compute the expected bytes and digests.

  /blob/<name>?size=N            plain 200; Range -> 206 (If-Range honoured);
                                 ETag and Last-Modified; If-None-Match /
                                 If-Modified-Since -> 304
  /cut/<name>?size=N&at=K&times=T   the first T requests send the headers
                                 (Content-Length N) and only K octets, then
                                 close (a dropped connection); later ones are
                                 /blob
  /norange/<name>?size=N         ignores Range: always 200 with everything
  /badrange/<name>?size=N        answers a Range with a 206 that starts
                                 somewhere else
  /corrupt/<name>?size=N&times=T the first T answers have one octet changed
  /flaky/<name>?size=N&fail=F[&ra=S]  F answers of 503 (Retry-After: S), then OK
  /status/<code>                 that status with a tiny body
  /slow/<name>?size=N&ms=M[&chunk=C]  the body in pieces of C octets with M ms
                                 between them (default 8192 / 20)
  /stall/<name>?size=N&at=K&ms=M the body stops after K octets for M ms
  /chunked/<name>?size=N         Transfer-Encoding: chunked
  /gz/<name>?size=N              Content-Encoding: gzip whatever was asked
  /redir/<name>?size=N           302 to /blob/<name>
  /mut/<name>?size=N             like /blob but the content depends on the
                                 version, which `POST /bump/<name>` raises
  /stats                         JSON: connections, requests, max parallel
  /log                           JSON: one record per request
  /reset                         forget counters and log
"""
import gzip
import hashlib
import http.server
import json
import socketserver
import sys
import threading
import time
import urllib.parse

LOCK = threading.Lock()
STATE = {
    'conns': 0, 'reqs': 0, 'active': 0, 'max_active': 0,
    'log': [], 'counters': {}, 'versions': {},
}


def content(name, size, version=0):
    out = bytearray()
    ctr = 0
    seed = ('%s#%d' % (name, version)).encode()
    while len(out) < size:
        out += hashlib.sha256(seed + ctr.to_bytes(8, 'little')).digest()
        ctr += 1
    return bytes(out[:size])


def rfc_date(t):
    return time.strftime('%a, %d %b %Y %H:%M:%S GMT', time.gmtime(t))


def count(key):
    with LOCK:
        STATE['counters'][key] = STATE['counters'].get(key, 0) + 1
        return STATE['counters'][key]


class H(http.server.BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    server_version = 'FakeDL/1'

    def log_message(self, *a):
        pass

    def setup(self):
        super().setup()
        with LOCK:
            STATE['conns'] += 1

    # ------------------------------------------------------------------
    def do_POST(self):
        n = int(self.headers.get('Content-Length') or 0)
        if n:
            self.rfile.read(n)
        u = urllib.parse.urlparse(self.path)
        if u.path.startswith('/bump/'):
            name = u.path[6:]
            with LOCK:
                STATE['versions'][name] = STATE['versions'].get(name, 0) + 1
            return self.small(200, b'ok')
        return self.small(404, b'no')

    def do_HEAD(self):
        return self.do_GET()

    def small(self, code, body, extra=None):
        self.send_response(code)
        self.send_header('Content-Type', 'text/plain')
        self.send_header('Content-Length', str(len(body)))
        for k, v in (extra or {}).items():
            self.send_header(k, v)
        self.end_headers()
        if self.command != 'HEAD':
            self.wfile.write(body)

    def do_GET(self):
        u = urllib.parse.urlparse(self.path)
        q = {k: v[0] for k, v in urllib.parse.parse_qs(u.query).items()}
        parts = u.path.split('/')
        kind = parts[1] if len(parts) > 1 else ''
        name = '/'.join(parts[2:])
        rec = {
            'path': self.path, 'range': self.headers.get('Range'),
            'if_range': self.headers.get('If-Range'),
            'inm': self.headers.get('If-None-Match'),
            'ims': self.headers.get('If-Modified-Since'),
            'conn': id(self.connection), 'ua': self.headers.get('User-Agent'),
            'ae': self.headers.get('Accept-Encoding'),
            't': time.time(),
        }
        with LOCK:
            if kind not in ('stats', 'log', 'reset'):
                STATE['reqs'] += 1
                STATE['log'].append(rec)
            if kind not in ('stats', 'log', 'reset'):
                STATE['active'] += 1
                STATE['max_active'] = max(STATE['max_active'], STATE['active'])
        try:
            self.route(kind, name, q, u)
        except (BrokenPipeError, ConnectionResetError):
            self.close_connection = True
        finally:
            with LOCK:
                if kind not in ('stats', 'log', 'reset'):
                    STATE['active'] -= 1

    # ------------------------------------------------------------------
    def route(self, kind, name, q, u):
        size = int(q.get('size', '0'))
        if kind == 'stats':
            with LOCK:
                s = {k: STATE[k] for k in ('conns', 'reqs', 'max_active')}
            return self.small(200, json.dumps(s).encode())
        if kind == 'log':
            with LOCK:
                return self.small(200, json.dumps(STATE['log']).encode())
        if kind == 'reset':
            with LOCK:
                STATE['conns'] = 0
                STATE['reqs'] = 0
                STATE['max_active'] = 0
                STATE['log'] = []
                STATE['counters'] = {}
            return self.small(200, b'ok')
        if kind == 'status':
            return self.small(int(name), b'status ' + name.encode())
        if kind == 'redir':
            return self.small(302, b'', {'Location': '/blob/%s?%s' % (name, u.query)})
        if kind == 'cut':
            n = count('cut:' + name)
            if n <= int(q.get('times', '1')):
                return self.cut(name, size, int(q['at']))
            kind = 'blob'
        if kind == 'flaky':
            n = count('flaky:' + name)
            if n <= int(q.get('fail', '1')):
                extra = {}
                if 'ra' in q:
                    extra['Retry-After'] = q['ra']
                return self.small(503, b'busy', extra)
            kind = 'blob'
        if kind == 'corrupt':
            n = count('corrupt:' + name)
            data = bytearray(content(name, size))
            if n <= int(q.get('times', '1000000')) and data:
                data[len(data) // 2] ^= 0x55
            return self.serve(bytes(data), name, ranges=True, version=0)
        if kind == 'norange':
            return self.serve(content(name, size), name, ranges=False)
        if kind == 'badrange':
            data = content(name, size)
            if self.headers.get('Range'):
                self.send_response(206)
                self.send_header('Content-Range', 'bytes 0-%d/%d' % (len(data) - 1, len(data)))
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)
                return
            return self.serve(data, name, ranges=True)
        if kind == 'slow' or kind == 'stall':
            return self.slow(name, size, q, kind)
        if kind == 'chunked':
            data = content(name, size)
            self.send_response(200)
            self.send_header('Transfer-Encoding', 'chunked')
            self.end_headers()
            i = 0
            step = 7000
            while i < len(data):
                piece = data[i:i + step]
                self.wfile.write(b'%x\r\n' % len(piece) + piece + b'\r\n')
                i += step
            self.wfile.write(b'0\r\n\r\n')
            return
        if kind == 'gz':
            body = gzip.compress(content(name, size))
            self.send_response(200)
            self.send_header('Content-Encoding', 'gzip')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if kind == 'mut':
            with LOCK:
                v = STATE['versions'].get(name, 0)
            return self.serve(content(name, size, v), name, ranges=True, version=v)
        if kind == 'blob':
            return self.serve(content(name, size), name, ranges=True)
        return self.small(404, b'unknown ' + kind.encode())

    # ------------------------------------------------------------------
    def validators(self, data, name, version):
        etag = '"%s-%d-%d"' % (hashlib.md5(name.encode()).hexdigest()[:8], len(data), version)
        lm = rfc_date(1700000000 + version * 86400)
        return etag, lm

    def serve(self, data, name, ranges, version=0):
        etag, lm = self.validators(data, name, version)
        inm = self.headers.get('If-None-Match')
        ims = self.headers.get('If-Modified-Since')
        if inm is not None and inm == etag or (inm is None and ims == lm):
            self.send_response(304)
            self.send_header('ETag', etag)
            self.send_header('Last-Modified', lm)
            self.end_headers()
            return
        rng = self.headers.get('Range')
        ifr = self.headers.get('If-Range')
        start, end = 0, len(data) - 1
        code = 200
        if rng and ranges and rng.startswith('bytes='):
            if ifr is not None and ifr != etag and ifr != lm:
                rng = None
            else:
                a, _, b = rng[6:].partition('-')
                s = int(a) if a else 0
                if s >= len(data):
                    self.send_response(416)
                    self.send_header('Content-Range', 'bytes */%d' % len(data))
                    self.send_header('Content-Length', '0')
                    self.end_headers()
                    return
                start = s
                end = int(b) if b else len(data) - 1
                code = 206
        body = data[start:end + 1]
        self.send_response(code)
        self.send_header('Content-Type', 'application/octet-stream')
        self.send_header('Content-Length', str(len(body)))
        self.send_header('ETag', etag)
        self.send_header('Last-Modified', lm)
        if ranges:
            self.send_header('Accept-Ranges', 'bytes')
        if code == 206:
            self.send_header('Content-Range', 'bytes %d-%d/%d' % (start, end, len(data)))
        self.end_headers()
        if self.command != 'HEAD':
            self.wfile.write(body)

    def cut(self, name, size, at):
        data = content(name, size)
        etag, lm = self.validators(data, name, 0)
        self.send_response(200)
        self.send_header('Content-Length', str(size))
        self.send_header('ETag', etag)
        self.send_header('Last-Modified', lm)
        self.send_header('Accept-Ranges', 'bytes')
        self.end_headers()
        self.wfile.write(data[:at])
        self.wfile.flush()
        self.close_connection = True
        try:
            self.connection.shutdown(2)
        except OSError:
            pass

    def slow(self, name, size, q, kind):
        data = content(name, size)
        etag, lm = self.validators(data, name, 0)
        rng = self.headers.get('Range')
        start = 0
        code = 200
        if rng and rng.startswith('bytes='):
            start = int(rng[6:].partition('-')[0])
            code = 206
        body = data[start:]
        self.send_response(code)
        self.send_header('Content-Length', str(len(body)))
        self.send_header('ETag', etag)
        self.send_header('Accept-Ranges', 'bytes')
        if code == 206:
            self.send_header('Content-Range', 'bytes %d-%d/%d' % (start, size - 1, size))
        self.end_headers()
        ms = int(q.get('ms', '20'))
        if kind == 'stall':
            at = int(q['at'])
            self.wfile.write(body[:at])
            self.wfile.flush()
            time.sleep(ms / 1000.0)
            self.wfile.write(body[at:])
            return
        chunk = int(q.get('chunk', '8192'))
        i = 0
        while i < len(body):
            self.wfile.write(body[i:i + chunk])
            self.wfile.flush()
            i += chunk
            time.sleep(ms / 1000.0)


class S(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True

    def handle_error(self, request, client_address):
        pass  # a client that closes or resets is part of the test
    request_queue_size = 256
    allow_reuse_address = True


if __name__ == '__main__':
    host = '127.0.0.1'
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    srv = S((host, port), H)
    print('port %d' % srv.server_address[1], flush=True)
    srv.serve_forever()
