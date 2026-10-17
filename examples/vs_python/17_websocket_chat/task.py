import base64
import os
import socket

port = int(os.environ["VSPY_PORT"])
sock = socket.create_connection(("127.0.0.1", port))
f = sock.makefile("rwb")
key = base64.b64encode(os.urandom(16)).decode()
f.write(f"GET /chat HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n"
        f"Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n".encode())
f.flush()
assert f.readline().startswith(b"HTTP/1.1 101")
while f.readline() != b"\r\n":
    pass


def send(op, data=b""):
    mask = os.urandom(4)
    f.write(bytes([0x80 | op, 0x80 | len(data)]) + mask + bytes(b ^ mask[i % 4] for i, b in enumerate(data)))
    f.flush()


def recv():
    n = f.read(2)[1] & 127
    return f.read(n).decode()


for msg in ("hello", "firn", "bye"):
    send(1, msg.encode())
    print(recv())
send(8)
