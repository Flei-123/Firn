#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/liveb4/server.py -- a REAL server for round B4.

The point of this file is that it is NOT the client. `lib/net/http.fi`
writes octets and reads octets; if the other end were written in the same
repository it would agree with itself about every misunderstanding. This one
is Python's own `http.server`, driven by a handful of routes that between
them exercise every framing rule the client claims to implement:

    /plain            200, Content-Length, text/html; charset=utf-8
    /chunked          200, Transfer-Encoding: chunked, with extensions and
                      a trailer section
    /gzip             200, Content-Encoding: gzip
    /gzip-chunked     200, both at once
    /deflate          200, Content-Encoding: deflate (zlib framed)
    /latin1           200, text/html; charset=ISO-8859-1
    /redir/<n>        302 to /redir/<n-1>, /plain at 0
    /see-other        303 to /echo     (a POST has to become a GET)
    /keep-method      307 to /echo     (a POST has to stay a POST)
    /loop-a /loop-b   302 at each other
    /tohttps          302 to an https: URL -- the TLS boundary, reached
                      through a redirect rather than typed in
    /echo             method, body and selected request headers back
    /cached           200 with Cache-Control: max-age and an ETag
    /etag             200 with an ETag; answers 304 to If-None-Match
    /nostore          200 with Cache-Control: no-store
    /cookie           200 with five Set-Cookie headers
    /status/<n>       the bare status code
    /slowhead         the whole answer dribbled out ONE OCTET PER WRITE
    /closebody        a body with no framing at all, ended by the close
    /notrailer        chunked with the last chunk missing -- a client that
                      reports success here is broken
    /shutdown         stops the server

It prints `port <n>` on standard output and then serves until killed, the
same shape as tools/net/echo.fi so the same shell helper waits for it.
"""
import gzip
import hashlib
import io
import socketserver
import sys
import threading
import zlib
from http.server import BaseHTTPRequestHandler

PLAIN = (
    b"<!doctype html><html><head><title>plain</title></head>"
    b"<body><p id=p>hello</p></body></html>"
)
ETAG = '"' + hashlib.sha256(PLAIN).hexdigest()[:16] + '"'
LASTMOD = "Wed, 21 Oct 2015 07:28:00 GMT"


class H(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "FirnB4Test/1"
    sys_version = ""

    def log_message(self, *a):
        pass

    def _raw(self, blob):
        self.wfile.write(blob)

    def _send(self, code, body=b"", ctype="text/html; charset=utf-8",
              extra=None, chunked=False, encoding=None, drop_last=False):
        if encoding == "gzip":
            body = gzip.compress(body, mtime=0)
        elif encoding == "deflate":
            body = zlib.compress(body)
        head = ["HTTP/1.1 %d %s" % (code, self.responses.get(code, ("x",))[0])]
        if ctype:
            head.append("Content-Type: " + ctype)
        if encoding:
            head.append("Content-Encoding: " + encoding)
        for k, v in (extra or []):
            head.append("%s: %s" % (k, v))
        if chunked:
            head.append("Transfer-Encoding: chunked")
        else:
            head.append("Content-Length: %d" % len(body))
        head.append("Connection: keep-alive")
        self._raw(("\r\n".join(head) + "\r\n\r\n").encode())
        if self.command == "HEAD":
            return
        if chunked:
            out = io.BytesIO()
            step = 17
            k = 0
            for i in range(0, len(body), step):
                part = body[i:i + step]
                ext = ";n=%d" % k if k % 3 == 0 else ""
                out.write(("%x%s\r\n" % (len(part), ext)).encode())
                out.write(part)
                out.write(b"\r\n")
                k += 1
            if not drop_last:
                out.write(b"0\r\nX-Trailer: yes\r\n\r\n")
            self._raw(out.getvalue())
        else:
            self._raw(body)

    def route(self):
        p = self.path.split("?")[0]
        body = b""
        if "Content-Length" in self.headers:
            body = self.rfile.read(int(self.headers["Content-Length"]))

        if p.startswith("/js/"):
            # The external scripts of the script-ordering test. Each one
            # only says its own name, so the ORDER is the whole answer.
            name = p[4:].split(".")[0]
            return self._send(200, ("print(\"%s\");" % name).encode(),
                              ctype="text/javascript")
        if p == "/plain":
            return self._send(200, PLAIN)
        if p == "/chunked":
            return self._send(200, PLAIN, chunked=True)
        if p == "/notrailer":
            return self._send(200, PLAIN, chunked=True, drop_last=True)
        if p == "/gzip":
            return self._send(200, PLAIN, encoding="gzip")
        if p == "/gzip-chunked":
            return self._send(200, PLAIN, encoding="gzip", chunked=True)
        if p == "/deflate":
            return self._send(200, PLAIN, encoding="deflate")
        if p == "/latin1":
            return self._send(200, "<p>Gruesse</p>".encode("latin-1"),
                              ctype="text/html; charset=ISO-8859-1")
        if p == "/big":
            return self._send(200, b"x" * 300000)
        if p.startswith("/redir/"):
            n = int(p.rsplit("/", 1)[1])
            target = "/plain" if n <= 0 else "/redir/%d" % (n - 1)
            return self._send(302, b"", extra=[("Location", target)])
        if p == "/deep/relredir":
            return self._send(302, b"", extra=[("Location", "../plain")])
        if p == "/see-other":
            return self._send(303, b"", extra=[("Location", "/echo")])
        if p == "/keep-method":
            return self._send(307, b"", extra=[("Location", "/echo")])
        if p == "/loop-a":
            return self._send(302, b"", extra=[("Location", "/loop-b")])
        if p == "/loop-b":
            return self._send(302, b"", extra=[("Location", "/loop-a")])
        if p == "/tohttps":
            # ROUND B5: this used to point at `example.invalid`, which was
            # refused before a socket was ever opened. The client now has
            # a resolver, so an invalid name comes back `Resolve` and no
            # longer says anything about TLS. It therefore points at THIS
            # server over `https://` -- the connection succeeds, the
            # handshake does not, and the refusal is a TLS one.
            return self._send(
                302, b"",
                extra=[("Location",
                        "https://" + self.headers.get("Host", "localhost")
                        + "/x")])
        if p == "/echo":
            out = ["METHOD=" + self.command,
                   "BODY=" + body.decode("latin-1")]
            for k in ("Content-Type", "Cookie", "Accept-Encoding",
                      "If-None-Match", "User-Agent", "Host", "Connection"):
                if k in self.headers:
                    out.append("%s=%s" % (k, self.headers[k]))
            return self._send(200, "\n".join(out).encode(),
                              ctype="text/plain; charset=utf-8")
        if p == "/cached":
            n = "60"
            if "max-age=" in self.path:
                n = self.path.split("max-age=")[-1].split("&")[0]
            return self._send(200, PLAIN, extra=[
                ("Cache-Control", "max-age=" + n), ("ETag", ETAG)])
        if p == "/nostore":
            return self._send(200, PLAIN,
                              extra=[("Cache-Control", "no-store")])
        if p == "/etag":
            if self.headers.get("If-None-Match") == ETAG:
                self._raw(("HTTP/1.1 304 Not Modified\r\nETag: %s\r\n"
                           "Connection: keep-alive\r\n\r\n" % ETAG).encode())
                return
            return self._send(200, PLAIN, extra=[("ETag", ETAG),
                                                 ("Last-Modified", LASTMOD)])
        if p == "/cookie":
            return self._send(200, b"ok", extra=[
                ("Set-Cookie", "sid=abc; Path=/"),
                ("Set-Cookie", "pref=dark; Path=/; Max-Age=600"),
                ("Set-Cookie", "gone=1; Path=/; Max-Age=0"),
                ("Set-Cookie", "deep=2; Path=/a/b"),
                ("Set-Cookie", "sec=3; Path=/; Secure"),
            ])
        if p.startswith("/status/"):
            return self._send(int(p.rsplit("/", 1)[1]), b"body")
        if p == "/slowhead":
            blob = ("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n"
                    "Content-Length: %d\r\nConnection: keep-alive\r\n\r\n"
                    % len(PLAIN)).encode() + PLAIN
            for i in range(len(blob)):
                self.wfile.write(blob[i:i + 1])
                self.wfile.flush()
            return
        if p == "/closebody":
            self._raw(b"HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n"
                      b"Connection: close\r\n\r\n" + PLAIN)
            self.close_connection = True
            return
        if p == "/shutdown":
            self._send(200, b"bye")
            threading.Thread(target=self.server.shutdown, daemon=True).start()
            return
        return self._send(404, b"no")

    do_GET = route
    do_POST = route
    do_HEAD = route


class S(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 0
    srv = S(("127.0.0.1", port), H)
    print("port %d" % srv.server_address[1], flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
