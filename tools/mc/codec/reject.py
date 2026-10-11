#!/usr/bin/env python3
"""Hostile frames: the Firn side must refuse them with a clean error and never hang/crash.
usage: reject.py <firn-binary>"""
import socket, subprocess, sys, zlib, struct

def varint(n):
    out = bytearray()
    while True:
        b = n & 0x7F; n >>= 7
        if n: out.append(b | 0x80)
        else: out.append(b); return bytes(out)

CASES = [  # name, threshold, bytes to send, expected error code (2 TooBig, 3 Inflate, 4 Malformed, 1 Closed)
    ("frame length above 2 MiB",          "-",  varint(3000000) + b"\x00" * 16, 2),
    ("frame length zero",                 "-",  varint(0), 4),
    ("varint that never ends",            "-",  b"\xff\xff\xff\xff\xff\xff\xff", 4),
    ("inflated size above the cap",       "256", varint(8) + varint(9000000) + b"\x78\x9c\x03\x00\x00\x00\x00\x01", 2),
    ("zlib stream with a bad checksum",   "256", None, 3),
    ("announced size != real size",       "256", None, 3),
    ("peer closes in the middle",         "-",  varint(100) + b"\x00" * 10, 1),
]

def build(name):
    if name == "zlib stream with a bad checksum":
        z = bytearray(zlib.compress(b"A" * 1000)); z[-1] ^= 1
        inner = varint(1000) + bytes(z)
        return varint(len(inner)) + inner
    if name == "announced size != real size":
        z = zlib.compress(b"A" * 1000)
        inner = varint(999) + z
        return varint(len(inner)) + inner

ok = True
for name, thr, data, want in CASES:
    if data is None:
        data = build(name)
    srv = socket.socket(); srv.bind(("127.0.0.1", 0)); srv.listen(1)
    p = subprocess.Popen([sys.argv[1], str(srv.getsockname()[1]), thr, "-", "reject"], stdout=subprocess.PIPE, text=True)
    c, _ = srv.accept()
    c.sendall(data)
    if want == 1:
        c.close()
    try:
        out, _ = p.communicate(timeout=20)
    except subprocess.TimeoutExpired:
        p.kill(); print("FAIL: hang on case:", name); ok = False; continue
    got = out.strip()
    if got != "error=%d" % want:
        print("FAIL: %s: got %r want error=%d" % (name, got, want)); ok = False
    else:
        print("  ok  refused: %s (%s)" % (name, got))
    c.close(); srv.close()
sys.exit(0 if ok else 1)
