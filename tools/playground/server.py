import json
import os
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

import config
import runtime
import sandbox

STATIC_DIR = os.path.join(config.HERE, "static")
EXAMPLES_DIR = os.path.join(config.HERE, "examples")
CONTENT_TYPES = {".html": "text/html; charset=utf-8", ".css": "text/css; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".json": "application/json"}
EXAMPLE_ORDER = ("hello", "vec", "strings", "fstring", "tuple", "timer")
EXAMPLE_TITLES = {
    "hello": "Hello", "vec": "Vec", "strings": "Strings", "fstring": "f-String",
    "tuple": "Tuple", "timer": "Timer / millis",
}
JOBS = threading.BoundedSemaphore(config.MAX_PARALLEL_JOBS)
CHECK_JOBS = threading.BoundedSemaphore(config.MAX_PARALLEL_CHECKS)
ENDPOINTS = {"/api/run": (JOBS, sandbox.execute), "/api/check": (CHECK_JOBS, sandbox.check)}


def load_examples():
    items = []
    for name in EXAMPLE_ORDER:
        with open(os.path.join(EXAMPLES_DIR, name + ".fi"), encoding="utf-8") as handle:
            items.append({"id": name, "title": EXAMPLE_TITLES[name], "source": handle.read()})
    return items


class Handler(BaseHTTPRequestHandler):
    server_version = "firn-playground"

    def log_message(self, fmt, *args):
        pass

    def send_body(self, status, content_type, body):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.send_header("X-Content-Type-Options", "nosniff")
        self.end_headers()
        self.wfile.write(body)

    def send_json(self, status, payload):
        self.send_body(status, "application/json", json.dumps(payload).encode("utf-8"))

    def do_GET(self):
        path = self.path.split("?", 1)[0]
        if path == "/api/examples":
            return self.send_json(200, load_examples())
        name = "index.html" if path == "/" else path.lstrip("/")
        full = os.path.realpath(os.path.join(STATIC_DIR, name))
        extension = os.path.splitext(full)[1]
        if not full.startswith(STATIC_DIR + os.sep) or extension not in CONTENT_TYPES or not os.path.isfile(full):
            return self.send_json(404, {"error": "not found"})
        with open(full, "rb") as handle:
            self.send_body(200, CONTENT_TYPES[extension], handle.read())

    def do_POST(self):
        endpoint = ENDPOINTS.get(self.path)
        if endpoint is None:
            return self.send_json(404, {"error": "not found"})
        slots, handler = endpoint
        length = int(self.headers.get("Content-Length") or 0)
        if length > config.MAX_SOURCE_BYTES * 2:
            return self.send_json(413, {"error": "request too large"})
        try:
            source = json.loads(self.rfile.read(length)).get("source")
        except (ValueError, AttributeError):
            return self.send_json(400, {"error": "invalid json"})
        if not slots.acquire(timeout=5):
            return self.send_json(503, {"error": "busy, try again"})
        try:
            self.send_json(200, handler(source))
        finally:
            slots.release()


def main():
    runtime.stage()
    server = ThreadingHTTPServer((config.HOST, config.PORT), Handler)
    server.daemon_threads = True
    print(f"firn playground on http://{config.HOST}:{config.PORT}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
