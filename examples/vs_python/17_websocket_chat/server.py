"""Harness only: a minimal WebSocket echo server ("echo: <text>") on 127.0.0.1:$VSPY_PORT."""
import base64, hashlib, os, socketserver

GUID = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11"


class H(socketserver.StreamRequestHandler):
    def handle(self):
        key = b""
        while True:
            line = self.rfile.readline()
            if line in (b"\r\n", b""):
                break
            if line.lower().startswith(b"sec-websocket-key:"):
                key = line.split(b":", 1)[1].strip()
        accept = base64.b64encode(hashlib.sha1(key + GUID).digest())
        self.wfile.write(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
                         b"Sec-WebSocket-Accept: " + accept + b"\r\n\r\n")
        while True:
            head = self.rfile.read(2)
            if len(head) < 2:
                return
            op, n = head[0] & 15, head[1] & 127
            if n == 126:
                n = int.from_bytes(self.rfile.read(2), "big")
            mask = self.rfile.read(4)
            data = bytes(b ^ mask[i % 4] for i, b in enumerate(self.rfile.read(n)))
            if op == 8:
                self.wfile.write(b"\x88\x00")
                return
            reply = b"echo: " + data
            self.wfile.write(bytes([0x81, len(reply)]) + reply)


class S(socketserver.ThreadingTCPServer):
    allow_reuse_address = True
    daemon_threads = True


S(("127.0.0.1", int(os.environ["VSPY_PORT"])), H).serve_forever()
