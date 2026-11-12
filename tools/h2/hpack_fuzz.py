import random
import subprocess
import sys

sys.path.insert(0, sys.argv[1])
from hpack import Encoder

tool = sys.argv[2]
runs = int(sys.argv[3])
rng = random.Random(7)
enc = Encoder()
crashes = 0
for i in range(runs):
    raw = b""
    for _ in range(rng.randint(1, 4)):
        data = enc.encode([("x-%d" % rng.randint(0, 9), "v" * rng.randint(0, 50)), (":path", "/a/b/%d" % i)])
        data = bytearray(data)
        for _ in range(rng.randint(0, 4)):
            if data:
                data[rng.randrange(len(data))] = rng.randrange(256)
        if rng.random() < 0.3 and data:
            data = data[:rng.randrange(len(data))]
        raw += len(data).to_bytes(4, "big") + bytes(data)
    p = subprocess.run([tool], input=b"D" + raw, capture_output=True, timeout=20)
    if p.returncode < 0 or p.returncode > 3:
        crashes += 1
print("fuzz runs", runs, "crashes", crashes)
sys.exit(1 if crashes else 0)
