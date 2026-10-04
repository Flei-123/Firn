#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""A local store server for the appkit end-to-end run.

Serves the files of a store directory (index.json, entry.json, speicher/...)
over plain HTTP and can misbehave on request, so the client's handling of a
bad network is measured, not assumed:

  --chunked          answer with Transfer-Encoding: chunked (1000 octet chunks)
  --slow-ms N        wait N ms between 16 KiB pieces of a body
  --cutoff PART      for a path containing PART: announce the full length,
                     send only half and hang up (a dropped connection)
  --redirect A=B     answer 302 -> B for path A (relative redirects)
  --log FILE         one line per request: "GET /path 200"

It prints "PORT <n>" on its first line (port 0 = pick a free one).
"""
import argparse
import http.server
import os
import socketserver
import time


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--root", required=True)
    ap.add_argument("--port", type=int, default=0)
    ap.add_argument("--chunked", action="store_true")
    ap.add_argument("--slow-ms", type=int, default=0)
    ap.add_argument("--cutoff", default="")
    ap.add_argument("--redirect", action="append", default=[])
    ap.add_argument("--log", default="")
    a = ap.parse_args()
    root = os.path.abspath(a.root)
    redirects = dict(r.split("=", 1) for r in a.redirect)

    class H(http.server.BaseHTTPRequestHandler):
        protocol_version = "HTTP/1.1"

        def log_message(self, *args):
            pass

        def note(self, code):
            if a.log:
                with open(a.log, "a") as f:
                    f.write("%s %s %d\n" % (self.command, self.path, code))

        def do_GET(self):
            path = self.path.split("?", 1)[0]
            if path in redirects:
                self.send_response(302)
                self.send_header("Location", redirects[path])
                self.send_header("Content-Length", "0")
                self.send_header("Connection", "close")
                self.end_headers()
                self.note(302)
                return
            rel = os.path.normpath(path.lstrip("/"))
            full = os.path.join(root, rel)
            if rel.startswith("..") or not os.path.isfile(full):
                self.send_response(404)
                self.send_header("Content-Length", "9")
                self.send_header("Connection", "close")
                self.end_headers()
                self.wfile.write(b"not found")
                self.note(404)
                return
            data = open(full, "rb").read()
            self.send_response(200)
            self.send_header("Content-Type", "application/octet-stream")
            self.send_header("Connection", "close")
            if a.chunked:
                self.send_header("Transfer-Encoding", "chunked")
            else:
                self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            cut = len(data)
            if a.cutoff and a.cutoff in path:
                cut = len(data) // 2
            try:
                if a.chunked:
                    for i in range(0, cut, 1000):
                        piece = data[i:min(i + 1000, cut)]
                        self.wfile.write(b"%x;ext=1\r\n" % len(piece) + piece + b"\r\n")
                        if a.slow_ms and (i // 1000) % 16 == 15:
                            time.sleep(a.slow_ms / 1000.0)
                    if cut == len(data):
                        self.wfile.write(b"0\r\nX-Trailer: 1\r\n\r\n")
                else:
                    for i in range(0, cut, 16384):
                        self.wfile.write(data[i:min(i + 16384, cut)])
                        self.wfile.flush()
                        if a.slow_ms:
                            time.sleep(a.slow_ms / 1000.0)
                self.note(200)
            except (BrokenPipeError, ConnectionResetError):
                self.note(0)
            self.close_connection = True

    class S(socketserver.ThreadingMixIn, http.server.HTTPServer):
        daemon_threads = True
        allow_reuse_address = True

    srv = S(("127.0.0.1", a.port), H)
    print("PORT %d" % srv.server_address[1], flush=True)
    srv.serve_forever()


if __name__ == "__main__":
    main()
