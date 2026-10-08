#!/usr/bin/env python3
"""tools/jsonsort_cross/cross.py -- json_write_opts against Python's json.dumps.

    cross.py gen   <ndocs> <seed> <in.bin> <meta.json>     random documents
    cross.py fuzz  <ndocs> <seed> <in.bin>                 mutated / junk documents (no expectation)
    cross.py check <in.bin> <out.bin> <meta.json>          compare byte for byte

`gen` writes ndocs random JSON documents (nested objects and arrays, strings
with quotes, backslashes, control characters, DEL, accents, CJK, characters
above the BMP, U+2028, keys that are prefixes of each other and keys whose
code point order differs from their UTF-16 order; integers up to the i64
limits; floats from random bit patterns, decimals, powers of ten, subnormals,
-0.0; empty containers) as text, written by json.dumps with random
ensure_ascii / indent / separators / sort_keys so that the PARSER sees many
layouts too. `check` re-reads the answers of jsonsort_cli (11 variants per
document, the keyword arguments of json.dumps below) and compares each with
json.dumps(json.loads(text), **kwargs) BYTE FOR BYTE.

Out of scope on purpose (documented in lib/std/json.fi, J3/J4/J7): integers
beyond the i64 range (Firn keeps a double), lone surrogates, duplicate keys.
Infinity is covered by its own family and the variant with JSON_PY_NONFINITE.
"""
import json
import random
import struct
import sys

# the keyword arguments of json.dumps for each variant of jsonsort_cli.fi
VARIANTS = [
    dict(),                                                         # 0 json.dumps(o)
    dict(sort_keys=True),                                           # 1
    dict(sort_keys=True, indent=2),                                 # 2
    dict(sort_keys=True, indent=4, ensure_ascii=False),             # 3
    dict(sort_keys=True, separators=(",", ":")),                    # 4
    dict(indent=0),                                                 # 5
    dict(sort_keys=True, indent="\t", ensure_ascii=False),          # 6
    dict(separators=(", ", ": "), indent=1),                        # 7
    dict(sort_keys=True, separators=(" , ", " : ")),                # 8
    dict(ensure_ascii=False),                                       # 9
    dict(sort_keys=True),                                           # 10 (+JSON_PY_NONFINITE): Infinity allowed
]

ALPHABETS = [
    "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 _-./:",
    "\"\\/\b\f\n\r\t",
    "".join(chr(c) for c in range(0, 32)) + "\x7f",
    "äöüßéèêñçøåÆ¿¡€£¥©®™",  # english: ok -- non-ASCII test data
    "日本語中文한국어ひらがなカタカナ",
    "\U0001F600\U0001F4A9\U0001F680\U00010348\U0001D11E\U0010FFFF",
    "   ​﻿�￿～퟿\u0080߿ࠀ",
]

SPECIAL_FLOATS = [0.0, -0.0, 0.1, 0.2, 0.1 + 0.2, 1.0, -1.0, 100.0, 123456789.0, 1e15, 1e16, 1e17,
                  9999999999999998.0, 1e22, 1e23, 1e-4, 1e-5, 1.5e-5, 1e-7, 1.2345678e-10, 5e-324,
                  2.2250738585072014e-308, 1.7976931348623157e308, 4.35, 0.5, 2.0 ** 53, 2.0 ** 63,
                  -2.0 ** 63, 12345678901234567.0, 0.3, 1 / 3, 2 / 3, 1e100, 1e-100, 123e-20]

SPECIAL_INTS = [0, 1, -1, 7, 255, 256, 65535, 2 ** 31 - 1, -2 ** 31, 2 ** 32, 2 ** 53 - 1, 2 ** 53,
                2 ** 53 + 1, -2 ** 53 - 1, 2 ** 63 - 1, -2 ** 63, 10 ** 15, 10 ** 18]


def rstr(rng, maxlen=12):
    n = rng.choice((0, 1, 1, 2, 3, 5, 8, maxlen))
    out = []
    for _ in range(n):
        alpha = rng.choice(ALPHABETS) if rng.random() < 0.5 else ALPHABETS[0]
        out.append(rng.choice(alpha))
    return "".join(out)


KEY_POOL = ["a", "ab", "abc", "b", "B", "A", "", "a\x00", "a b", "ä", "z", "～", "\U0001F600",
            "\u007f", "\u0080", "aa", "a~", "~", "~a", "Z", "_", "0", "10", "9", "key", "Key"]


def rkey(rng):
    if rng.random() < 0.5:
        return rng.choice(KEY_POOL)
    return rstr(rng, 6)


def rfloat(rng):
    r = rng.random()
    if r < 0.2:
        return rng.choice(SPECIAL_FLOATS)
    if r < 0.5:
        while True:
            x = struct.unpack("<d", struct.pack("<Q", rng.getrandbits(64)))[0]
            if x == x and abs(x) != float("inf"):
                return x
    if r < 0.7:
        return round(rng.uniform(-1000, 1000), rng.randint(0, 6))
    if r < 0.85:
        return rng.choice((-1, 1)) * 10 ** rng.uniform(-30, 30)
    return float(rng.randint(-10 ** 6, 10 ** 6)) * rng.choice((1.0, 1e10, 1e-10, 1e15, 1e-5))


def rint(rng):
    r = rng.random()
    if r < 0.3:
        return rng.choice(SPECIAL_INTS)
    if r < 0.6:
        return rng.randint(-1000, 1000)
    if r < 0.8:
        return rng.randint(-2 ** 63, 2 ** 63 - 1)
    return rng.randint(-10 ** 9, 10 ** 9)


def rvalue(rng, depth):
    if depth <= 0:
        kinds = ("null", "bool", "int", "float", "str")
    else:
        kinds = ("null", "bool", "int", "float", "str", "arr", "obj", "arr", "obj")
    k = rng.choice(kinds)
    if k == "null":
        return None
    if k == "bool":
        return rng.random() < 0.5
    if k == "int":
        return rint(rng)
    if k == "float":
        return rfloat(rng)
    if k == "str":
        return rstr(rng)
    if k == "arr":
        return [rvalue(rng, depth - 1) for _ in range(rng.choice((0, 0, 1, 2, 3, 5, 9)))]
    d = {}
    for _ in range(rng.choice((0, 0, 1, 2, 3, 5, 8, 20))):
        d[rkey(rng)] = rvalue(rng, depth - 1)
    return d


def render(rng, obj):
    kw = {}
    kw["ensure_ascii"] = rng.random() < 0.5
    r = rng.random()
    if r < 0.3:
        kw["indent"] = rng.choice((0, 1, 2, 4, "\t"))
    elif r < 0.5:
        kw["separators"] = (",", ":")
    kw["sort_keys"] = rng.random() < 0.3
    return json.dumps(obj, **kw).encode("utf-8")


def write_docs(docs, path):
    words = [len(docs)]
    out = bytearray()
    for d in docs:
        out += struct.pack("<Q", len(d)) + d + b"\0" * ((8 - len(d) % 8) % 8)
    with open(path, "wb") as f:
        f.write(struct.pack("<Q", len(docs)) + bytes(out))


def gen(ndocs, seed, path, meta):
    rng = random.Random(seed)
    docs, kinds = [], []
    for i in range(ndocs):
        r = rng.random()
        if r < 0.03:
            # non-finite family: Python parses 1e999 as inf
            obj_text = "[" + ", ".join(rng.choice(("1e999", "-1e999", "1.5", "0")) for _ in range(rng.randint(1, 4))) + \
                       ', {"b": 1e999, "a": [-1e999]}]'
            docs.append(obj_text.encode())
            kinds.append("nonfinite")
        elif r < 0.10:
            # deep nesting (Firn limit is 200)
            depth = rng.randint(30, 120)
            obj = rvalue(rng, 1)
            for _ in range(depth):
                obj = rng.choice(([obj], {rkey(rng): obj, "z": 0, "a": [1]}))
            docs.append(render(rng, obj))
            kinds.append("deep")
        else:
            obj = rvalue(rng, rng.randint(1, 6))
            docs.append(render(rng, obj))
            kinds.append("normal")
    write_docs(docs, path)
    json.dump(kinds, open(meta, "w"))


def mutate(rng, d):
    b = bytearray(d)
    for _ in range(rng.choice((1, 1, 2, 4))):
        if not b:
            break
        op = rng.randint(0, 5)
        i = rng.randrange(len(b))
        if op == 0:
            b[i] = rng.randrange(256)
        elif op == 1:
            del b[i:i + rng.randint(1, 8)]
        elif op == 2:
            b[i:i] = bytes(rng.randrange(256) for _ in range(rng.randint(1, 6)))
        elif op == 3:
            b = b[:i]
        elif op == 4:
            j = rng.randrange(len(b))
            b[i:i] = b[j:j + rng.randint(1, 20)]
        else:
            b[i:i + 1] = rng.choice((b"\\u", b"\\ud800", b"\\udc00\\ud800", b'"', b"{", b"]", b",", b"\\", b"e5", b"-", b"\x00", b"\xff", b"\xc3", b"\xed\xa0\x80"))
    return bytes(b)


def fuzz(ndocs, seed, path):
    rng = random.Random(seed)
    docs = []
    while len(docs) < ndocs:
        obj = rvalue(rng, rng.randint(1, 4))
        base = render(rng, obj)
        docs.append(base)                    # valid: the round trip runs on it
        for _ in range(3):
            docs.append(mutate(rng, base))   # mostly invalid: no crash, no hang
        if rng.random() < 0.1:
            docs.append(bytes(rng.randrange(256) for _ in range(rng.randint(0, 40))))
    write_docs(docs[:ndocs], path)


def check(inpath, outpath, metapath):
    raw = open(inpath, "rb").read()
    n = struct.unpack_from("<Q", raw, 0)[0]
    pos, docs = 8, []
    for _ in range(n):
        ln = struct.unpack_from("<Q", raw, pos)[0]
        docs.append(raw[pos + 8:pos + 8 + ln])
        pos += 8 + ((ln + 7) // 8) * 8
    kinds = json.load(open(metapath))
    out = open(outpath, "rb").read()
    pos = 0
    bad = 0
    compared = 0
    per_variant = [0] * len(VARIANTS)
    floats = ints = strs = objs = 0
    for di, text in enumerate(docs):
        obj = json.loads(text.decode("utf-8"))
        kind = kinds[di]
        for v, kw in enumerate(VARIANTS):
            status, ln = struct.unpack_from("<QQ", out, pos)
            pos += 16
            got = out[pos:pos + ln]
            pos += ((ln + 7) // 8) * 8
            if kind == "nonfinite" and v != 10:
                continue   # null instead of Infinity unless JSON_PY_NONFINITE (documented)
            want = json.dumps(obj, **kw).encode("utf-8")
            compared += 1
            if status != 0 or got != want:
                bad += 1
                per_variant[v] += 1
                if bad <= 6:
                    print("  MISMATCH doc #%d (%s) variant %d %r status=%d" % (di, kind, v, kw, status))
                    k = 0
                    while k < min(len(got), len(want)) and got[k] == want[k]:
                        k += 1
                    print("    first difference at octet %d:" % k)
                    print("      ours : %r" % got[max(0, k - 30):k + 40])
                    print("      json : %r" % want[max(0, k - 30):k + 40])
        stack = [obj]
        while stack:
            x = stack.pop()
            if isinstance(x, float):
                floats += 1
            elif isinstance(x, int) and not isinstance(x, bool):
                ints += 1
            elif isinstance(x, str):
                strs += 1
            elif isinstance(x, dict):
                objs += 1
                stack.extend(x.values())
                strs += len(x)
            elif isinstance(x, list):
                stack.extend(x)
    print("jsonsort cross-check: %d documents (%d normal, %d deep, %d non-finite), %d outputs compared byte for byte"
          % (n, kinds.count("normal"), kinds.count("deep"), kinds.count("nonfinite"), compared))
    print("  content: %d objects, %d floats, %d integers, %d strings/keys; input bytes %d" %
          (objs, floats, ints, strs, sum(len(d) for d in docs)))
    print("  mismatches: %d  (per variant: %s)" % (bad, per_variant))
    print("OK" if bad == 0 else "FAIL")
    return 0 if bad == 0 else 1


if __name__ == "__main__":
    a = sys.argv
    if len(a) >= 6 and a[1] == "gen":
        gen(int(a[2]), int(a[3]), a[4], a[5])
    elif len(a) >= 5 and a[1] == "fuzz":
        fuzz(int(a[2]), int(a[3]), a[4])
    elif len(a) >= 5 and a[1] == "check":
        sys.exit(check(a[2], a[3], a[4]))
    else:
        print(__doc__)
        sys.exit(2)
