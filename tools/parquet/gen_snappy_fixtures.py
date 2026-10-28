import sys, os, random
import pyarrow as pa
out = sys.argv[1]
os.makedirs(out, exist_ok=True)
random.seed(7)
words = "der die das und ist nicht ein eine zu den mit auf für von im dem sich auch es an wird werden aus hat bei nach wie über so zum".split()
text = " ".join(random.choice(words) for _ in range(30000)).encode()
rnd = bytes(random.getrandbits(8) for _ in range(5000))
runs = b"a" * 1000 + b"ab" * 700 + b"xyz" * 300
cases = {"text": text, "rnd": rnd, "runs": runs, "empty": b"", "one": b"q", "mix": rnd + text[:7000] + runs + rnd}
for name, data in cases.items():
    open(f"{out}/snappy_{name}.raw", "wb").write(data)
    comp = pa.compress(data, codec="snappy", asbytes=True) if data else b"\x00"
    open(f"{out}/snappy_{name}.snappy", "wb").write(comp)
