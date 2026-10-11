#!/usr/bin/env python3
"""Independent peer for the mc_codec cross-check: its own VarInt, framing, zlib and
AES-CFB8 (python `cryptography`). It sends a stream of packets to the Firn program,
which echoes each back, and verifies the echo byte for byte.

usage: peer.py <firn-binary> <threshold|-> <encrypt:0|1> <seed>
"""
import os, random, socket, struct, subprocess, sys, zlib
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

def varint(n):
    n &= 0xFFFFFFFF
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)

class Stream:
    def __init__(self, sock, key):
        self.s = sock
        self.enc = self.dec = None
        if key:
            self.enc = Cipher(algorithms.AES(key), modes.CFB8(key)).encryptor()
            self.dec = Cipher(algorithms.AES(key), modes.CFB8(key)).decryptor()
        self.buf = b""
    def send(self, data, rng=None):
        if self.enc:
            data = self.enc.update(data)
        if rng is None:
            self.s.sendall(data)
        else:  # random split writes: TCP has no message boundaries
            i = 0
            while i < len(data):
                k = rng.choice([1, 2, 3, 7, 100, 1000, 65536])
                self.s.sendall(data[i:i + k])
                i += k
    def need(self, n):
        while len(self.buf) < n:
            d = self.s.recv(1 << 20)
            if not d:
                raise EOFError
            if self.dec:
                d = self.dec.update(d)
            self.buf += d
    def read_varint(self):
        r = 0; sh = 0
        while True:
            self.need(1)
            b = self.buf[0]; self.buf = self.buf[1:]
            r |= (b & 0x7F) << sh; sh += 7
            if not b & 0x80:
                return r
    def read_frame(self, thr):
        ln = self.read_varint()
        self.need(ln)
        fr = self.buf[:ln]; self.buf = self.buf[ln:]
        if thr is None:
            return fr
        # compressed framing
        i = 0; dl = 0; sh = 0
        while True:
            b = fr[i]; i += 1
            dl |= (b & 0x7F) << sh; sh += 7
            if not b & 0x80:
                break
        return fr[i:] if dl == 0 else zlib.decompress(fr[i:])

def frame(payload, thr, rng):
    """payload = id + body"""
    if thr is None:
        return varint(len(payload)) + payload
    if len(payload) >= thr:
        z = zlib.compress(payload, rng.choice([0, 1, 6, 9]))
        inner = varint(len(payload)) + z
    else:
        inner = varint(0) + payload
    return varint(len(inner)) + inner

def make_body(rng, n):
    kind = rng.choice(["random", "zeros", "text", "mixed"])
    if kind == "random":
        return rng.randbytes(n)
    if kind == "zeros":
        return bytes(n)
    if kind == "text":
        return (b"minecraft:stone minecraft:dirt " * (n // 30 + 1))[:n]
    return bytes((i * 7 + (i >> 8)) & 255 for i in range(n))

def main():
    binary, thr_s, enc, seed = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
    thr = None if thr_s == "-" else int(thr_s)
    rng = random.Random(seed)
    key = rng.randbytes(16) if enc else None
    srv = socket.socket(); srv.bind(("127.0.0.1", 0)); srv.listen(1)
    port = srv.getsockname()[1]
    args = [binary, str(port), "-" if thr is None else str(thr), key.hex() if key else "-", "echo"]
    proc = subprocess.Popen(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    srv.settimeout(20)
    conn, _ = srv.accept()
    conn.settimeout(60)
    st = Stream(conn, key)
    t = thr if thr is not None else 0
    sizes = [0, 1, 2, 15, 100, max(0, t - 2), max(0, t - 1), t, t + 1, t + 2, 255, 256, 257, 1000, 4096, 65535, 65536, 70000, 300000, 1900000]
    sizes = [s for s in sizes if s >= 0]
    count = 0
    for rnd in range(3):
        for sz in sizes:
            # keep incompressible bodies under the 2 MiB frame limit of the protocol
            pid = rng.choice([0, 1, 0x24, 0x28, 0x7F, 0x80, 300])
            body = make_body(rng, sz)
            payload = varint(pid) + body
            st.send(frame(payload, thr, rng), rng if rnd == 2 else None)
            echo = st.read_frame(thr)
            if echo != payload:
                print("FAIL: echo differs (size %d id %d thr %s enc %d)" % (sz, pid, thr_s, enc)); proc.kill(); sys.exit(1)
            count += 1
    conn.shutdown(socket.SHUT_WR)
    out, err = proc.communicate(timeout=60)
    if proc.returncode != 0 or ("packets=%d " % count) not in out:
        print("FAIL: firn side rc=%s out=%r err=%r" % (proc.returncode, out, err)); sys.exit(1)
    print("ok thr=%s enc=%d: %d packets echoed byte-identical; firn: %s" % (thr_s, enc, count, out.strip()))

main()
