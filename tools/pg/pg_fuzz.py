import random
import socket
import struct
import subprocess
import sys

client = sys.argv[1]
runs = int(sys.argv[2])
rng = random.Random(5)


def msg(kind, payload):
    return kind + struct.pack(">I", len(payload) + 4) + payload


def row_desc():
    cols = rng.randint(0, 3)
    p = struct.pack(">H", cols)
    for i in range(cols):
        p += b"c%d\0" % i + struct.pack(">IHIhIH", 0, 0, 23, 4, 0xFFFFFFFF, 0)
    return p


def data_row():
    cols = rng.randint(0, 3)
    p = struct.pack(">H", cols)
    for _ in range(cols):
        if rng.random() < 0.2:
            p += struct.pack(">i", -1)
        else:
            v = b"x" * rng.randint(0, 5)
            p += struct.pack(">I", len(v)) + v
    return p


def mutate(b):
    b = bytearray(b)
    for _ in range(rng.randint(0, 3)):
        if b:
            b[rng.randrange(len(b))] = rng.randrange(256)
    if rng.random() < 0.2 and b:
        b = b[:rng.randrange(len(b))]
    return bytes(b)


def random_message():
    k = rng.choice([b"T", b"D", b"D", b"C", b"E", b"N", b"Z", b"1", b"2", b"n", b"S", b"R", b"G", b"H", b"X", bytes([rng.randrange(256)])])
    if k == b"T":
        p = row_desc()
    elif k == b"D":
        p = data_row()
    elif k == b"C":
        p = b"SELECT 1\0"
    elif k == b"E":
        p = b"SERROR\0C42601\0Mbad\0\0"
    elif k == b"Z":
        p = b"I"
    else:
        p = bytes(rng.randrange(256) for _ in range(rng.randint(0, 12)))
    m = msg(k, mutate(p) if rng.random() < 0.6 else p)
    if rng.random() < 0.1:
        m = k + struct.pack(">I", rng.choice([0, 1, 3, 4, 0xFFFFFFFF, 0x7FFFFFFF])) + m[5:]
    return m


crashes = 0
for i in range(runs):
    srv = socket.socket()
    srv.bind(("127.0.0.1", 0))
    srv.listen(1)
    port = srv.getsockname()[1]
    p = subprocess.Popen([client], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    p.stdin.write(str(port).encode())
    p.stdin.close()
    conn, _ = srv.accept()
    conn.settimeout(2)
    try:
        conn.recv(4096)
        conn.sendall(msg(b"R", struct.pack(">I", 0)) + msg(b"Z", b"I"))
        for _ in range(rng.randint(1, 10)):
            conn.sendall(random_message())
    except Exception:
        pass
    conn.close()
    srv.close()
    try:
        rc = p.wait(timeout=10)
        if rc < 0 or rc > 3:
            crashes += 1
    except subprocess.TimeoutExpired:
        p.kill()
        crashes += 1
print("pg fuzz runs", runs, "crashes", crashes)
sys.exit(1 if crashes else 0)
