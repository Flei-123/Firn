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
    kind = rng.randrange(5)
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
    else:
        for _ in range(rng.randrange(1, 10)):
            i = rng.randrange(len(b))
            b[i] = b[i] ^ (1 << rng.randrange(8))
    return bytes(b)


def mutate_xml(xml, rng):
    kind = rng.randrange(8)
    s = xml
    if kind == 0:
        return s[: rng.randrange(1, len(s) + 1)]
    if kind == 1:
        tags = list(re.finditer(rb"<[^>]+>", s))
        if tags:
            m = rng.choice(tags)
            return s[: m.start()] + s[m.end() :]
    elif kind == 2:
        tags = list(re.finditer(rb"<w:[a-zA-Z]+", s))
        if tags:
            m = rng.choice(tags)
            return s[: m.start()] + s[m.start() : m.end()] * rng.randrange(2, 30) + s[m.end() :]
    elif kind == 3:
        nums = list(re.finditer(rb'w:(?:w|val|h|left|right|top|bottom|line|before|after|sz|firstLine|hanging)="-?\d+"', s))
        if nums:
            m = rng.choice(nums)
            key = m.group(0).split(b"=")[0]
            val = rng.choice([b"-1", b"0", b"99999999999", b"-99999999999", b"2147483648", b"1e308", b"abc", b""])
            return s[: m.start()] + key + b'="' + val + b'"' + s[m.end() :]
    elif kind == 4:
        i = rng.randrange(len(s))
        return s[:i] + s[i : i + 400] * rng.randrange(2, 8) + s[i:]
    elif kind == 5:
        tags = list(re.finditer(rb"<w:[a-zA-Z]+", s))
        if tags:
            m = rng.choice(tags)
            return s[: m.start()] + b"<w:tbl><w:tr><w:tc>" * rng.randrange(1, 40) + s[m.start() :]
    elif kind == 6:
        b = bytearray(s)
        for _ in range(rng.randrange(1, 30)):
            b[rng.randrange(len(b))] = rng.randrange(256)
        return bytes(b)
    i = rng.randrange(len(s))
    return s[:i] + bytes(rng.randrange(256) for _ in range(rng.randrange(1, 32))) + s[i:]


def rewrite_zip(data, rng):
    zin = zipfile.ZipFile(io.BytesIO(data))
    out = io.BytesIO()
    names = zin.namelist()
    xml_names = [n for n in names if n.endswith(".xml") or n.endswith(".rels")]
    target = rng.choice(xml_names)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zout:
        for n in names:
            body = zin.read(n)
            if n == target and body:
                body = mutate_xml(body, rng)
            zout.writestr(n, body)
    return out.getvalue()


def deep_nesting(rng):
    depth = rng.choice([50, 500, 5000, 50000])
    tag = rng.choice([b"w:tbl", b"w:p", b"w:r", b"w:sdt", b"w:tc", b"mc:AlternateContent"])
    body = b"<%s>" % tag * depth + b"<w:t>x</w:t>" + b"</%s>" % tag * depth
    return body


def bomb(rng):
    size = rng.choice([20, 60, 200]) * 1024 * 1024
    return b"<w:p><w:r><w:t>" + b"a" * size + b"</w:t></w:r></w:p>"


def special_docs(data, rng):
    zin = zipfile.ZipFile(io.BytesIO(data))
    out = io.BytesIO()
    which = rng.randrange(3)
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as zout:
        for n in zin.namelist():
            body = zin.read(n)
            if n == "word/document.xml":
                head = body[: body.index(b"<w:body>") + 8]
                tail = body[body.index(b"</w:body>") :]
                if which == 0:
                    body = head + deep_nesting(rng) + tail
                elif which == 1:
                    body = head + bomb(rng) + tail
                else:
                    body = head + b"<w:p><w:r><w:t>x</w:t></w:r></w:p>" * rng.randrange(1, 60000) + tail
            zout.writestr(n, body)
    return out.getvalue()


def run_one(args):
    binary, path, mode = args
    cmd = [binary, mode, path] + (["0", "0.5"] if mode == "r" else [])
    try:
        r = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=60)
    except subprocess.TimeoutExpired:
        return "hang", path
    if r.returncode < 0:
        return "crash", path
    if r.returncode == 101:
        return "panic", path
    return "ok", path


def main():
    binary = sys.argv[1]
    folder = sys.argv[2]
    runs = int(sys.argv[3])
    keep = sys.argv[4] if len(sys.argv) > 4 else None
    seed = int(os.environ.get("FUZZ_SEED", "7"))
    rng = random.Random(seed)
    files = [os.path.join(folder, n) for n in sorted(os.listdir(folder)) if n.endswith(".docx")]
    datas = [open(f, "rb").read() for f in files]
    datas = [d for d in datas if 0 < len(d) < 400000]
    work = tempfile.mkdtemp()
    jobs = []
    for i in range(runs):
        base = rng.choice(datas)
        roll = rng.randrange(20)
        try:
            if roll < 2:
                data = mutate_bytes(base, rng)
            elif roll == 2:
                data = special_docs(base, rng)
            else:
                data = rewrite_zip(base, rng)
        except Exception:
            data = mutate_bytes(base, rng)
        path = os.path.join(work, "m%d.docx" % i)
        with open(path, "wb") as f:
            f.write(data)
        jobs.append((binary, path, "r" if i % 3 == 0 else "t"))
    counts = {"ok": 0, "crash": 0, "hang": 0, "panic": 0}
    bad = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as ex:
        for status, path in ex.map(run_one, jobs):
            counts[status] += 1
            if status != "ok":
                bad.append((status, path))
            else:
                os.unlink(path)
    print("runs=%d ok=%d crash=%d hang=%d panic=%d" % (runs, counts["ok"], counts["crash"], counts["hang"], counts["panic"]))
    for status, path in bad:
        if keep:
            os.makedirs(keep, exist_ok=True)
            os.replace(path, os.path.join(keep, "%s_%s" % (status, os.path.basename(path))))
        print("BAD", status, path)
    if not bad:
        os.rmdir(work)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
