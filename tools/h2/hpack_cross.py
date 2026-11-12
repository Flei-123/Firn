import os
import random
import subprocess
import sys

sys.path.insert(0, sys.argv[1])
from hpack import Decoder, Encoder

tool = sys.argv[2]
rng = random.Random(int(sys.argv[3]) if len(sys.argv) > 3 else 1)
NAMES = [":method", ":path", ":scheme", ":authority", "accept", "user-agent", "cookie",
         "content-type", "x-custom", "x-trace-id", "accept-encoding", "authorization", "etag"]


def rand_value():
    n = rng.choice([0, 1, 5, 20, 80, 300])
    alphabet = "abcdefghijklmnopqrstuvwxyz0123456789-_/.=;, ~\x7f"
    return "".join(rng.choice(alphabet) for _ in range(n))


def rand_blocks(count):
    blocks = []
    for _ in range(count):
        block = []
        for _ in range(rng.randint(1, 12)):
            block.append((rng.choice(NAMES), rand_value()))
        blocks.append(block)
    return blocks


def run(mode, payload):
    return subprocess.run([tool], input=mode + payload, capture_output=True, check=True).stdout


def check_decode(total):
    enc = Encoder()
    blocks = rand_blocks(total)
    raw = b""
    for b in blocks:
        data = enc.encode(b)
        raw += len(data).to_bytes(4, "big") + data
    out = run(b"D", raw).decode("latin1")
    parts = out.split("\n\n")
    got = []
    for part in parts[:-1] if parts[-1] == "" else parts:
        got.append([tuple(l.split("\t", 1)) for l in part.split("\n") if l != ""])
    want = [[(n, v) for n, v in b] for b in blocks]
    got_n = [[(n, v) for n, v in g] for g in got]
    return sum(1 for a, b in zip(want, got_n) if a == b), len(blocks)


def check_encode(total):
    blocks = rand_blocks(total)
    text = b""
    for b in blocks:
        for n, v in b:
            text += (("!" if n in ("authorization", "cookie") and rng.random() < 0.5 else "") + n + "\t" + v + "\n").encode("latin1")
        text += b"\n"
    out = run(b"E", text)
    dec = Decoder()
    pos = 0
    ok = 0
    for b in blocks:
        n = int.from_bytes(out[pos:pos + 4], "big")
        data = out[pos + 4:pos + 4 + n]
        pos += 4 + n
        got = [(a.decode("latin1") if isinstance(a, bytes) else a, v.decode("latin1") if isinstance(v, bytes) else v) for a, v in dec.decode(data, raw=False)]
        if got == b:
            ok += 1
    return ok, len(blocks)


a = check_decode(300)
b = check_encode(300)
print("decode", a, "encode", b)
sys.exit(0 if a[0] == a[1] and b[0] == b[1] else 1)
