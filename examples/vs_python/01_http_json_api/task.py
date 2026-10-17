import json
import threading
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, HTTPServer

items = []


class Handler(BaseHTTPRequestHandler):
    def reply(self, status, obj):
        body = json.dumps(obj).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/items":
            self.reply(200, items)
        else:
            self.reply(404, {"error": "not found"})

    def do_POST(self):
        try:
            item = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
        except ValueError:
            return self.reply(400, {"error": "bad json"})
        items.append(item)
        self.reply(201, {"id": len(items) - 1})

    def log_message(self, *args):
        pass


server = HTTPServer(("127.0.0.1", 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
base = "http://127.0.0.1:%d" % server.server_port


def call(path, data=None):
    req = urllib.request.Request(base + path, data=data, headers={"Content-Type": "application/json"})
    try:
        resp = urllib.request.urlopen(req)
    except urllib.error.HTTPError as e:
        resp = e
    print(resp.status, resp.read().decode())


call("/items", b'{"name": "apple", "qty": 3}')
call("/items", b'{"name": "pear", "qty": 5}')
call("/items")
call("/nope")
call("/items", b"{oops")
