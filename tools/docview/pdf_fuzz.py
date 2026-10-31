#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import concurrent.futures
import os
import random
import subprocess
import sys
import tempfile


def mutate(data, rng):
    kind = rng.randrange(6)
    b = bytearray(data)
    if kind == 0:
        return bytes(b[: rng.randrange(1, len(b) + 1)])
    if kind == 1:
        for _ in range(rng.randrange(1, 40)):
            b[rng.randrange(len(b))] = rng.randrange(256)
    elif kind == 2:
        i = rng.randrange(len(b))
        del b[i : i + rng.randrange(1, 200)]
    elif kind == 3:
        i = rng.randrange(len(b))
        b[i:i] = bytes(rng.randrange(256) for _ in range(rng.randrange(1, 64)))
    elif kind == 4:
        i = rng.randrange(len(b))
        j = rng.randrange(len(b))
        b[i : i + 32] = b[j : j + 32]
    else:
        for _ in range(rng.randrange(1, 10)):
            i = rng.randrange(len(b))
            b[i] = b[i] ^ (1 << rng.randrange(8))
    return bytes(b)


def run_one(args):
    binary, path, mode = args
    try:
        r = subprocess.run([binary, mode, path, "0", "0.5", "/dev/null"], capture_output=True, timeout=30)
    except subprocess.TimeoutExpired:
        return "hang", path
    if r.returncode < 0:
        return "crash", path
    return "ok", path


def main():
    binary = sys.argv[1]
    folder = sys.argv[2]
    runs = int(sys.argv[3])
    keep = sys.argv[4] if len(sys.argv) > 4 else None
    rng = random.Random(7)
    files = [os.path.join(folder, n) for n in sorted(os.listdir(folder)) if n.endswith(".pdf")]
    datas = [open(f, "rb").read() for f in files]
    datas = [d for d in datas if 0 < len(d) < 400000]
    work = tempfile.mkdtemp()
    jobs = []
    for i in range(runs):
        data = mutate(rng.choice(datas), rng)
        path = os.path.join(work, "m%d.pdf" % i)
        with open(path, "wb") as f:
            f.write(data)
        jobs.append((binary, path, "render" if i % 3 else "text"))
    counts = {"ok": 0, "crash": 0, "hang": 0}
    bad = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as ex:
        for status, path in ex.map(run_one, jobs):
            counts[status] += 1
            if status != "ok":
                bad.append(path)
    print("runs", runs, counts)
    if bad and keep:
        os.makedirs(keep, exist_ok=True)
        for p in bad[:20]:
            os.replace(p, os.path.join(keep, os.path.basename(p)))
    for p in os.listdir(work):
        try:
            os.remove(os.path.join(work, p))
        except OSError:
            pass
    os.rmdir(work)


main()
