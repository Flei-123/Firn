#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""tools/compress/http_check.py -- net.http's Content-Encoding handling against a
real HTTP server (python's http.server): gzip, deflate, br and zstd bodies,
the Accept-Encoding the client sends, and refusals (a damaged br/zstd body, an
encoding it cannot undo, a bomb above the 256 MiB cap is not tried here).

    http_check.py <http_main binary>

http_main is lib/net/http_main.fi built for the host (or a Windows build with
RUNNER=wine). One job per line on its standard input; see its header.
"""
import os, sys, subprocess, threading, zlib, gzip, http.server, socketserver, re

BIN = os.path.abspath(sys.argv[1])
RUNNER = os.environ.get("RUNNER", "").split()
import brotli

TEXT = (b"The quick brown fox jumps over the lazy dog. " * 40 + b"Firn speaks several compressions.\n" * 20)
BODIES = {}
BODIES["/gz"] = ("gzip", gzip.compress(TEXT))
BODIES["/deflate-zlib"] = ("deflate", zlib.compress(TEXT))
c = zlib.compressobj(6, zlib.DEFLATED, -15)
BODIES["/deflate-raw"] = ("deflate", c.compress(TEXT) + c.flush())
BODIES["/br"] = ("br", brotli.compress(TEXT, quality=9))
BODIES["/zstd"] = ("zstd", subprocess.run(["zstd", "-q", "-c", "-3"], input=TEXT, capture_output=True, check=True).stdout)
BODIES["/identity"] = ("identity", TEXT)
BODIES["/br-bad"] = ("br", brotli.compress(TEXT, quality=5)[:-9] + b"\x00" * 9)
BODIES["/zstd-bad"] = ("zstd", subprocess.run(["zstd", "-q", "-c", "-3"], input=TEXT, capture_output=True, check=True).stdout[:-30])
BODIES["/odd"] = ("compress", TEXT)
SEEN = []


class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *a):
        pass

    def do_GET(self):
        SEEN.append(self.headers.get("Accept-Encoding", ""))
        if self.path in BODIES:
            enc, body = BODIES[self.path]
            self.send_response(200)
            self.send_header("Content-Type", "text/plain")
            if enc != "identity":
                self.send_header("Content-Encoding", enc)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self.send_response(404)
            self.send_header("Content-Length", "0")
            self.end_headers()


class S(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True


srv = S(("127.0.0.1", 0), H)
port = srv.server_address[1]
threading.Thread(target=srv.serve_forever, daemon=True).start()

jobs = "".join("G http://127.0.0.1:%d%s\n" % (port, p) for p in BODIES)
r = subprocess.run(RUNNER + [BIN], input=jobs.encode(), capture_output=True, timeout=120)
out = r.stdout.decode(errors="replace")
blocks = out.split("\n.\n")
fails = []
want_text = TEXT.decode().replace("\\", "\\\\").replace("\n", "\\n")
results = {}
for path, blk in zip(BODIES, blocks):
    results[path] = blk
good = ["/gz", "/deflate-zlib", "/deflate-raw", "/br", "/zstd", "/identity"]
for p in good:
    blk = results.get(p, "")
    m = re.search(r"^BODY (\d+)$", blk, re.M)
    t = re.search(r"^BODYTEXT (.*)$", blk, re.M)
    if not (re.search(r"^STATUS 200$", blk, re.M) and m and int(m.group(1)) == len(TEXT) and t and t.group(1) == want_text):
        fails.append("%s: %s" % (p, blk[:200].replace("\n", " | ")))
for p in ("/br-bad", "/zstd-bad", "/odd"):
    blk = results.get(p, "")
    if "ERR Encoding" not in blk:
        fails.append("%s should be ERR Encoding: %s" % (p, blk[:200].replace("\n", " | ")))
if not SEEN or not all("br" in s and "zstd" in s and "gzip" in s for s in SEEN):
    fails.append("Accept-Encoding sent: %r" % SEEN[:3])
print("  http: %d requests, Accept-Encoding %r" % (len(SEEN), SEEN[0] if SEEN else ""))
for f in fails:
    print("  FAIL", f)
print("http checks: %d, failures: %d" % (len(good) + 3 + 1, len(fails)))
sys.exit(1 if fails else 0)
