import random
import socket
import subprocess
import sys

client = sys.argv[1]
runs = int(sys.argv[2])
rng = random.Random(11)
SAMPLE_BLOCKS = [bytes.fromhex("88"), bytes.fromhex("8886"), bytes.fromhex("4803343031"), bytes.fromhex("0f0d03313030")]


def frame(ftype, flags, sid, payload):
    return len(payload).to_bytes(3, "big") + bytes([ftype, flags]) + sid.to_bytes(4, "big") + payload


def random_frame():
    ftype = rng.choice([0, 1, 1, 2, 3, 4, 4, 5, 6, 7, 8, 9, 10, 200])
    flags = rng.choice([0, 1, 4, 5, 8, 12, 32, rng.randrange(256)])
    sid = rng.choice([0, 1, 1, 1, 2, 3, rng.randrange(1 << 31)])
    kind = rng.random()
    if kind < 0.4:
        payload = rng.choice(SAMPLE_BLOCKS)
    elif kind < 0.7:
        payload = bytes(rng.randrange(256) for _ in range(rng.choice([0, 1, 4, 5, 6, 8, 12, 30, 200])))
    else:
        payload = (rng.randrange(1 << 31)).to_bytes(4, "big")
    return frame(ftype, flags, sid, payload)


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
        data = frame(4, 0, 0, b"")
        for _ in range(rng.randint(1, 8)):
            if rng.random() < 0.2:
                f = bytearray(random_frame())
                f = f[:rng.randrange(len(f))]
                data += bytes(f)
            else:
                data += random_frame()
        conn.sendall(data)
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
print("h2 fuzz runs", runs, "crashes", crashes)
sys.exit(1 if crashes else 0)
