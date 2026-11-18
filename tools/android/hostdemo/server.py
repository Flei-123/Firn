#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import json
import sys
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

JSON_BODY = {"hello": "firn", "answer": 42}
SSE_FIRST = b"event: hello\ndata: first\n\n"
SSE_SECOND = b"data: second\n\n"
SSE_KEEPALIVE = b": ping\n\n"
SSE_STEP_SECONDS = 0.5
SSE_KEEPALIVE_SECONDS = 2.0


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, fmt, *args):
        print("%s %s" % (self.command, self.path), flush=True)

    def reply(self, status, ctype, body):
        self.send_response(status)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/json":
            self.reply(200, "application/json", json.dumps(JSON_BODY, separators=(",", ":")).encode())
        elif self.path == "/events":
            self.stream_events()
        else:
            self.reply(404, "text/plain", b"not found")

    def do_POST(self):
        size = int(self.headers.get("Content-Length", "0"))
        body = self.rfile.read(size)
        self.reply(200, "text/plain", b"echo:" + body)

    def stream_events(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.end_headers()
        try:
            self.wfile.write(SSE_FIRST)
            self.wfile.flush()
            time.sleep(SSE_STEP_SECONDS)
            self.wfile.write(SSE_SECOND)
            self.wfile.flush()
            while True:
                time.sleep(SSE_KEEPALIVE_SECONDS)
                self.wfile.write(SSE_KEEPALIVE)
                self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            return


def main():
    port = int(sys.argv[1])
    server = ThreadingHTTPServer(("0.0.0.0", port), Handler)
    print("listening %d" % port, flush=True)
    server.serve_forever()


main()
