"""Harness only: /n/<i> answers after 50 ms with 37*i+1 octets (so 20 requests in parallel take ~50 ms, serial 1 s)."""
import http.server, os, time


class H(http.server.BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def do_GET(self):
        time.sleep(0.05)
        body = b"x" * (37 * int(self.path.rsplit("/", 1)[1]) + 1)
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *a):
        pass


http.server.ThreadingHTTPServer(("127.0.0.1", int(os.environ["VSPY_PORT"])), H).serve_forever()
