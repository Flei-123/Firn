#!/usr/bin/env python3
import os, socket, struct, sys, time, json

def varint(n):
    out = b""
    while True:
        b = n & 0x7F
        n >>= 7
        out += bytes([b | (0x80 if n else 0)])
        if not n:
            return out

def read_varint(sock):
    n = 0
    for i in range(5):
        b = sock.recv(1)
        if not b:
            raise EOFError
        n |= (b[0] & 0x7F) << (7 * i)
        if not b[0] & 0x80:
            return n
    raise ValueError

def packet(pid, payload=b""):
    body = varint(pid) + payload
    return varint(len(body)) + body

def string(s):
    raw = s.encode()
    return varint(len(raw)) + raw

def recv_exact(sock, n):
    data = b""
    while len(data) < n:
        chunk = sock.recv(n - len(data))
        if not chunk:
            raise EOFError
        data += chunk
    return data

def read_packet(sock):
    length = read_varint(sock)
    return recv_exact(sock, length)

session = {}
for line in open(os.environ["MCPLAY_SESSION_FILE"]):
    if "=" in line:
        k, v = line.strip().split("=", 1)
        session[k] = v
host, port, proto = session["host"], int(session["port"]), int(session["protocol"])
result = {"host": host, "port": port, "env_version": os.environ.get("MC_VANILLA_VERSION")}

s = socket.create_connection((host, port), timeout=20)
s.sendall(packet(0, varint(proto) + string(host) + struct.pack(">H", port) + varint(1)))
s.sendall(packet(0))
data = read_packet(s)
i = 0
pid = data[0]
status_len = 0
shift = 0
j = 1
while True:
    b = data[j]; j += 1
    status_len |= (b & 0x7F) << shift
    shift += 7
    if not b & 0x80:
        break
status = json.loads(data[j:j + status_len])
result["status_version"] = status["version"]["name"]
result["status_protocol"] = status["version"]["protocol"]
s.close()

s = socket.create_connection((host, port), timeout=20)
s.sendall(packet(0, varint(proto) + string(host) + struct.pack(">H", port) + varint(2)))
s.sendall(packet(0, string(session["name"]) + (b"\x00" if proto >= 761 else b"")))
compressed = False
login = None
for _ in range(5):
    data = read_packet(s)
    if compressed:
        data = data[1:]
    pid = data[0]
    if pid == 3:
        compressed = True
        continue
    login = pid
    break
result["login_packet"] = login
s.close()
open(os.environ["FLSP_RESULT"], "w").write(json.dumps(result))
time.sleep(1.0)
