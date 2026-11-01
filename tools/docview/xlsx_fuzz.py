#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
import concurrent.futures
import io
import os
import random
import re
import subprocess
import sys
import tempfile
import zipfile


def mutate_bytes(data, rng):
    b = bytearray(data)
    kind = rng.randrange(4)
    if kind == 0:
        return bytes(b[: rng.randrange(1, len(b) + 1)])
    if kind == 1:
        for _ in range(rng.randrange(1, 40)):
            b[rng.randrange(len(b))] = rng.randrange(256)
    elif kind == 2:
        i = rng.randrange(len(b))
        del b[i : i + rng.randrange(1, 200)]
    else:
        i = rng.randrange(len(b))
        b[i:i] = bytes(rng.randrange(256) for _ in range(rng.randrange(1, 64)))
    return bytes(b)


def mutate_xml(xml, rng):
    kind = rng.randrange(6)
    if kind == 0:
        return xml[: rng.randrange(1, len(xml) + 1)]
    if kind == 1:
        tags = list(re.finditer(rb"<[^>]+>", xml))
        if tags:
            m = rng.choice(tags)
            return xml[: m.start()] + xml[m.end():]
    elif kind == 2:
        nums = list(re.finditer(rb'="[0-9.\-]+[a-z%]*"', xml))
        if nums:
            m = rng.choice(nums)
            val = rng.choice([b'"99999999999"', b'"-1"', b'"0"', b'"1e9cm"', b'"%"', b'"2147483648"'])
            return xml[: m.start() + 1] + val + xml[m.end():]
    elif kind == 3:
        tags = list(re.finditer(rb"<[a-z]+:[a-zA-Z\-]+", xml))
        if tags:
            m = rng.choice(tags)
            return xml[: m.start()] + xml[m.start():m.end()] * rng.randrange(2, 30) + xml[m.end():]
    elif kind == 4:
        return b"<row><c r=\"A1\">" * rng.randrange(100, 3000) + xml
    return mutate_bytes(xml, rng)


def mutate_zip(data, rng):
    zin = zipfile.ZipFile(io.BytesIO(data))
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w") as zo:
        for info in zin.infolist():
            body = zin.read(info.filename)
            if info.filename.endswith(".xml") and rng.random() < 0.6:
                body = mutate_xml(body, rng)
            zo.writestr(info.filename, body)
    return out.getvalue()


def run_one(args):
    exe, seed_file, seed, work = args
    rng = random.Random(seed)
    data = open(seed_file, "rb").read()
    mode = rng.randrange(3)
    data = mutate_bytes(data, rng) if mode == 0 else mutate_zip(data, rng)
    path = os.path.join(work, "m%d.xlsx" % seed)
    open(path, "wb").write(data)
    try:
        r = subprocess.run([exe, "t", path], capture_output=True, timeout=30)
        code = r.returncode
    except subprocess.TimeoutExpired:
        code = -999
    os.remove(path)
    return seed, seed_file, code


def main():
    exe, fixtures, runs = sys.argv[1], sys.argv[2], int(sys.argv[3])
    seeds = [os.path.join(fixtures, f) for f in sorted(os.listdir(fixtures)) if f.endswith(".xlsx")]
    bad = 0
    with tempfile.TemporaryDirectory() as work:
        jobs = [(exe, seeds[i % len(seeds)], 1000 + i, work) for i in range(runs)]
        with concurrent.futures.ThreadPoolExecutor(4) as ex:
            for seed, f, code in ex.map(run_one, jobs):
                if code not in (0, 4):
                    bad += 1
                    print("BAD", seed, os.path.basename(f), code)
    print("runs", runs, "bad", bad)
    sys.exit(1 if bad else 0)


main()
