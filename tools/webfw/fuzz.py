#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import os, random, socket, subprocess, sys, time

BIN, WORK, SEED, RUNS = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
os.makedirs(WORK, exist_ok=True)
rnd = random.Random(SEED)
PORT = 21000 + rnd.randrange(0, 900)
open(os.path.join(WORK, "a.txt"), "w").write("x" * 100)
p = subprocess.Popen([BIN, str(PORT), WORK], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
p.stdout.readline()
time.sleep(0.2)

SEEDS = [
    b"GET /hello/x HTTP/1.1\r\nHost: a\r\n\r\n",
    b"GET /static/a.txt HTTP/1.1\r\nHost: a\r\nRange: bytes=1-5\r\nIf-None-Match: W/\"1-2\"\r\n\r\n",
    b"POST /users HTTP/1.1\r\nHost: a\r\nContent-Length: 22\r\nContent-Type: application/json\r\n\r\n{\"name\":\"a\",\"age\":1}\n",
    b"POST /form HTTP/1.1\r\nHost: a\r\nContent-Length: 11\r\n\r\nx=%41&y=%zz",
    b"GET /session HTTP/1.1\r\nHost: a\r\nCookie: wf_sid=" + b"a" * 32 + b"." + b"b" * 64 + b"\r\n\r\n",
    b"GET /api/secret HTTP/1.1\r\nHost: a\r\nAuthorization: Basic YWRtaW46cHc=\r\n\r\n",
    b"OPTIONS /users HTTP/1.1\r\nHost: a\r\nOrigin: http://example.test\r\nAccess-Control-Request-Method: POST\r\n\r\n",
    b"GET /big HTTP/1.1\r\nHost: a\r\nAccept-Encoding: zstd, gzip\r\n\r\n",
    b"PUT /echo/a/b/c HTTP/1.1\r\nHost: a\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\n\r\n",
    b"GET /?who=%3Cb%3E&admin=1 HTTP/1.1\r\nHost: a\r\n\r\n",
]
TOKENS = [b"%", b"%00", b"%zz", b"..", b"/", b"//", b"\\", b"\x00", b"\xff\xfe", b"*", b":", b"bytes=-", b"bytes=9-1", b"bytes=0-99999999999999999999", b"{{", b"}}", b"\r\n", b"A" * 5000, b"?", b"&", b"=", b"Basic ", b"Bearer "]


def mutate(b):
    b = bytearray(b)
    for _ in range(rnd.randrange(1, 5)):
        k = rnd.randrange(6)
        i = rnd.randrange(len(b) + 1)
        if k == 0 and b:
            b[min(i, len(b) - 1)] ^= 1 << rnd.randrange(8)
        elif k == 1:
            b[i:i] = rnd.choice(TOKENS)
        elif k == 2 and b:
            del b[min(i, len(b) - 1): min(i, len(b) - 1) + rnd.randrange(1, 20)]
        elif k == 3:
            b = b[:i]
        elif k == 4 and b:
            j = rnd.randrange(len(b))
            b[j:j + 1] = bytes([rnd.randrange(256)])
        else:
            b[i:i] = bytes(rnd.randrange(256) for _ in range(rnd.randrange(1, 40)))
    return bytes(b)


def send(data):
    try:
        s = socket.create_connection(("127.0.0.1", PORT), timeout=2)
        s.sendall(data)
        s.settimeout(0.05)
        try:
            s.recv(65536)
        except socket.timeout:
            pass
        s.close()
    except OSError:
        pass


def alive():
    try:
        s = socket.create_connection(("127.0.0.1", PORT), timeout=3)
        s.sendall(b"GET /hello/ok HTTP/1.1\r\nHost: a\r\nConnection: close\r\n\r\n")
        s.settimeout(3)
        out = b""
        while True:
            d = s.recv(4096)
            if not d:
                break
            out += d
        return out.endswith(b"hello ok")
    except OSError:
        return False


bad = 0
for n in range(RUNS):
    send(mutate(rnd.choice(SEEDS)))
    if n % 200 == 199 and (p.poll() is not None or not alive()):
        bad += 1
        print("  server down after run", n)
        break
up = p.poll() is None and alive()
p.kill()
p.wait()
print("webfw fuzz: seed %d runs %d crashes %d" % (SEED, RUNS, 0 if (up and bad == 0) else 1))
sys.exit(0 if up and bad == 0 else 1)
