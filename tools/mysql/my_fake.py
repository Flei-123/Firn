import hashlib
import random
import socket
import struct
import subprocess
import sys

mode = sys.argv[1]
client = sys.argv[2]
runs = int(sys.argv[3]) if len(sys.argv) > 3 else 1
rng = random.Random(3)
PASSWORD = b"s3cret"
SALT = bytes(range(1, 21))


def packet(seq, payload):
    return struct.pack("<I", len(payload))[:3] + bytes([seq & 255]) + payload


def read_packet(conn):
    h = b""
    while len(h) < 4:
        d = conn.recv(4 - len(h))
        if not d:
            return None, None
        h += d
    n = h[0] | h[1] << 8 | h[2] << 16
    body = b""
    while len(body) < n:
        d = conn.recv(n - len(body))
        if not d:
            return None, None
        body += d
    return h[3], body


def handshake(plugin, salt):
    caps = 0xF7FF | (0x81 << 16)
    p = b"\x0a" + b"10.0.0-fake\0" + struct.pack("<I", 7) + salt[:8] + b"\0"
    p += struct.pack("<H", caps & 0xFFFF) + bytes([45]) + struct.pack("<H", 2) + struct.pack("<H", caps >> 16)
    p += bytes([21]) + bytes(10) + salt[8:] + b"\0" + plugin + b"\0"
    return packet(0, p)


def sha2_token(salt):
    h1 = hashlib.sha256(PASSWORD).digest()
    h2 = hashlib.sha256(h1).digest()
    h3 = hashlib.sha256(h2 + salt).digest()
    return bytes(a ^ b for a, b in zip(h1, h3))


def native_token(salt):
    h1 = hashlib.sha1(PASSWORD).digest()
    h2 = hashlib.sha1(h1).digest()
    h3 = hashlib.sha1(salt + h2).digest()
    return bytes(a ^ b for a, b in zip(h1, h3))


def token_of(body):
    pos = 32
    pos = body.index(b"\0", pos) + 1
    n = body[pos]
    return body[pos + 1:pos + 1 + n]


def ok(seq):
    return packet(seq, b"\x00\x00\x00\x02\x00\x00\x00")


def serve_auth(conn):
    if mode == "sha2":
        conn.sendall(handshake(b"caching_sha2_password", SALT))
        seq, body = read_packet(conn)
        good = token_of(body) == sha2_token(SALT)
        if not good:
            return False
        conn.sendall(packet(2, b"\x01\x03") + ok(3))
    else:
        conn.sendall(handshake(b"mysql_native_password", SALT))
        seq, body = read_packet(conn)
        if token_of(body) != native_token(SALT):
            return False
        salt2 = bytes(range(50, 70))
        conn.sendall(packet(2, b"\xfe" + b"caching_sha2_password\0" + salt2 + b"\0"))
        seq, body = read_packet(conn)
        if body != sha2_token(salt2):
            return False
        conn.sendall(ok(4))
    seq, body = read_packet(conn)
    if body is None or body[0] != 3:
        return False
    conn.sendall(ok(1))
    return True


def mutate(b):
    b = bytearray(b)
    for _ in range(rng.randint(0, 4)):
        if b:
            b[rng.randrange(len(b))] = rng.randrange(256)
    if rng.random() < 0.2 and b:
        b = b[:rng.randrange(len(b))]
    return bytes(b)


def fuzz_serve(conn):
    hs = handshake(b"mysql_native_password", SALT)
    conn.sendall(mutate(hs) if rng.random() < 0.3 else hs)
    seq, body = read_packet(conn)
    if body is None:
        return
    conn.sendall(ok(2))
    for i in range(rng.randint(1, 4)):
        seq, body = read_packet(conn)
        if body is None:
            return
        for k in range(rng.randint(1, 8)):
            payload = bytes(rng.randrange(256) for _ in range(rng.choice([0, 1, 2, 7, 9, 20, 40])))
            if rng.random() < 0.5:
                payload = bytes([rng.choice([0, 1, 2, 5, 0xfe, 0xff, 0xfb])]) + payload
            conn.sendall(packet(rng.choice([k + 1, k + 1, k + 2, 0]), payload))


crashes = 0
fails = 0
for i in range(runs):
    srv = socket.socket()
    srv.bind(("127.0.0.1", 0))
    srv.listen(1)
    port = srv.getsockname()[1]
    p = subprocess.Popen([client], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    p.stdin.write(str(port).encode())
    p.stdin.close()
    conn, _ = srv.accept()
    conn.settimeout(1)
    good = False
    try:
        if mode == "fuzz":
            fuzz_serve(conn)
        else:
            good = serve_auth(conn)
    except Exception:
        pass
    conn.close()
    srv.close()
    try:
        rc = p.wait(timeout=10)
        if rc < 0 or rc > 3:
            crashes += 1
        if mode != "fuzz" and (rc != 0 or not good):
            fails += 1
    except subprocess.TimeoutExpired:
        p.kill()
        crashes += 1
print("mysql", mode, "runs", runs, "crashes", crashes, "failures", fails)
sys.exit(1 if crashes or fails else 0)
