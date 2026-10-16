#!/usr/bin/env python3
"""Cross-check of auth.otp: RFC 4226 / 6238 test vectors, Python hmac+hashlib,
base64.b32*, and an independent otpauth:// parser built on urllib.

usage: otp_cross.py OTPTOOL CASES SEED     (exit 1 on any difference)
"""
import base64
import hashlib
import hmac
import random
import struct
import subprocess
import sys
import urllib.parse

tool, cases, seed = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
rnd = random.Random(seed)
HASH = {0: hashlib.sha1, 1: hashlib.sha256, 2: hashlib.sha512}
NAME = {0: "sha1", 1: "sha256", 2: "sha512"}


def hotp(alg, key, counter, digits):
    mac = hmac.new(key, struct.pack(">Q", counter), HASH[alg]).digest()
    off = mac[-1] & 15
    v = struct.unpack(">I", mac[off:off + 4])[0] & 0x7FFFFFFF
    return str(v % (10 ** digits)).zfill(digits)


def totp(alg, key, t, step, t0, digits):
    return hotp(alg, key, (t - t0) // step if t >= t0 else 0, digits)


lines = []
want = []

# --- RFC 4226 Appendix D (SHA-1, 6 digits) and RFC 6238 Appendix B (8 digits)
k20 = b"12345678901234567890"
rfc4226 = ["755224", "287082", "359152", "969429", "338314", "254676", "287922", "162583", "399871", "520489"]
for c, w in enumerate(rfc4226):
    lines.append("H 0 %s %d 6" % (k20.hex(), c))
    want.append(w)
keys = {0: k20, 1: b"12345678901234567890123456789012", 2: b"1234567890123456789012345678901234567890123456789012345678901234"}
rfc6238 = {59: ("94287082", "46119246", "90693936"), 1111111109: ("07081804", "68084774", "25091201"),
           1111111111: ("14050471", "67062674", "99943326"), 1234567890: ("89005924", "91819424", "93441116"),
           2000000000: ("69279037", "90698825", "38618901"), 20000000000: ("65353130", "77737706", "47863826")}
for t, ws in rfc6238.items():
    for alg in range(3):
        lines.append("T %d %s %d 30 0 8" % (alg, keys[alg].hex(), t))
        want.append(ws[alg])
n_vectors = len(lines)

# --- random HOTP / TOTP against hmac+hashlib
for _ in range(cases):
    alg = rnd.randrange(3)
    klen = rnd.choice([1, 5, 10, 16, 20, 32, 63, 64, 65, 100, 128, 129, 200])
    key = bytes(rnd.randrange(256) for _ in range(klen))
    digits = rnd.randrange(1, 11)
    if rnd.random() < 0.5:
        c = rnd.choice([0, 1, rnd.randrange(1 << 20), rnd.randrange(1 << 63), (1 << 64) - 1])
        lines.append("H %d %s %d %d" % (alg, key.hex(), c, digits))
        want.append(hotp(alg, key, c, digits))
    else:
        t = rnd.choice([0, 29, 30, 59, rnd.randrange(1 << 33)])
        step = rnd.choice([1, 15, 30, 60, 90])
        t0 = rnd.choice([0, 0, 0, 100, rnd.randrange(1000)])
        lines.append("T %d %s %d %d %d %d" % (alg, key.hex(), t, step, t0, digits))
        want.append(totp(alg, key, t, step, t0, digits))

# --- Base32
n_hotp = len(lines)
for _ in range(cases // 2):
    raw = bytes(rnd.randrange(256) for _ in range(rnd.randrange(0, 41)))
    lines.append("E %s" % raw.hex())
    e = base64.b32encode(raw).decode()
    want.append(e + " " + e.rstrip("="))
ALPHA = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"


def ref_decode(s):
    t = s.replace(" ", "").replace("-", "").replace("=", "").replace("\t", "")
    for ch in t:
        if ch.upper() not in ALPHA or ord(ch) > 127:
            return "ERR"
    if len(t) % 8 in (1, 3, 6):
        return "ERR"
    t = t.upper()
    t += "=" * (-len(t) % 8)
    try:
        return base64.b32decode(t).hex()
    except Exception:
        return "ERR"


for _ in range(cases // 2):
    raw = bytes(rnd.randrange(256) for _ in range(rnd.randrange(0, 30)))
    s = base64.b32encode(raw).decode()
    r = rnd.random()
    if r < 0.2:
        s = s.lower()
    elif r < 0.4:
        s = s.rstrip("=")
    elif r < 0.55:
        s = " ".join(s[i:i + 4] for i in range(0, len(s), 4))
    elif r < 0.7:
        s = s[:rnd.randrange(len(s) + 1)]
    elif r < 0.85:
        i = rnd.randrange(len(s) + 1)
        s = s[:i] + rnd.choice("1089!@/_zZ\u00e4") + s[i:]
    if not s.strip():
        s = "A"
    lines.append("D %s" % s)
    want.append(ref_decode(s))
n_b32 = len(lines)

# --- otpauth URIs: independent reference parser
def ref_uri(u):
    pr = urllib.parse.urlsplit(u)
    if pr.scheme.lower() != "otpauth":
        return "bad"
    kind = pr.netloc.lower()
    if kind not in ("totp", "hotp"):
        return "bad"
    label = urllib.parse.unquote(pr.path[1:], errors="surrogateescape")
    issuer = ""
    if ":" in label:
        issuer, account = label.split(":", 1)
        account = account.lstrip(" ")
    else:
        account = label
    alg, digits, period, counter, secret = 0, 6, 30, 0, None
    have_counter = False
    for part in pr.query.split("&"):
        if not part:
            continue
        k, _, v = part.partition("=")
        k = k.lower()
        v = urllib.parse.unquote_plus(v, errors="surrogateescape")
        if k == "secret":
            d = ref_decode(v)
            if d == "ERR" or d == "":
                return "bad"
            secret = d
        elif k == "issuer":
            issuer = v
        elif k == "algorithm":
            m = {"sha1": 0, "sha256": 1, "sha512": 2}.get(v.lower())
            if m is None:
                return "bad"
            alg = m
        elif k == "digits":
            if not v.isascii() or not v.isdigit() or not (1 <= int(v) <= 10) or len(v) > 19:
                return "bad"
            digits = int(v)
        elif k == "period":
            if not v.isascii() or not v.isdigit() or int(v) < 1 or len(v) > 19:
                return "bad"
            period = int(v)
        elif k == "counter":
            if not v.isascii() or not v.isdigit() or len(v) > 19:
                return "bad"
            counter = int(v)
            have_counter = True
    if secret is None:
        return "bad"
    if kind == "hotp" and not have_counter:
        return "bad"
    ib = issuer.encode("utf-8", "surrogateescape").hex()
    ab = account.encode("utf-8", "surrogateescape").hex()
    return "ok %d %d %d %d %d %s %s|%s" % (0 if kind == "totp" else 1, alg, digits, period, counter, secret, ib, ab)


def rand_text():
    return "".join(rnd.choice("abcXYZ09 :@.-_~+%é€/") for _ in range(rnd.randrange(0, 8)))


for _ in range(cases // 2):
    raw = bytes(rnd.randrange(256) for _ in range(rnd.randrange(1, 25)))
    sec = base64.b32encode(raw).decode().rstrip("=")
    params = ["secret=" + sec]
    if rnd.random() < 0.5:
        params.append("issuer=" + urllib.parse.quote_plus(rand_text()))
    if rnd.random() < 0.4:
        params.append("algorithm=" + rnd.choice(["SHA1", "SHA256", "SHA512", "sha256", "MD5"]))
    if rnd.random() < 0.4:
        params.append("digits=" + rnd.choice(["6", "7", "8", "10", "0", "11", "x", "08"]))
    if rnd.random() < 0.4:
        params.append("period=" + rnd.choice(["30", "60", "0", "1", "abc", "15"]))
    if rnd.random() < 0.5:
        params.append("counter=" + rnd.choice(["0", "5", "123456", "-1", "z"]))
    if rnd.random() < 0.1:
        params.append("image=https://x/y.png")
    if rnd.random() < 0.05:
        params.remove(params[0])
    rnd.shuffle(params)
    label = urllib.parse.quote(rand_text(), safe=":") if rnd.random() < 0.7 else urllib.parse.quote(rand_text())
    kind = rnd.choice(["totp", "totp", "hotp", "TOTP", "foo"])
    scheme = rnd.choice(["otpauth", "otpauth", "OTPAUTH", "http"])
    u = "%s://%s/%s?%s" % (scheme, kind, label, "&".join(params))
    lines.append("U " + u)
    want.append(ref_uri(u))
n_uri = len(lines)

# --- builder + round trip
for _ in range(cases // 4):
    raw = bytes(rnd.randrange(256) for _ in range(rnd.randrange(1, 25)))
    iss = rnd.choice(["", "Example", "Ex ample", "Caf\u00e9 Co", "a&b=c"])
    acc = rnd.choice(["alice", "a@b.c", "x y", "\u00fc\u00f1i/z", "p+q%"])
    alg = rnd.randrange(3)
    digits = rnd.randrange(1, 11)
    period = rnd.choice([1, 30, 60])
    counter = rnd.randrange(1 << 40)
    kind = rnd.randrange(2)
    lines.append("B %d %s %s %s %d %d %d %d" % (kind, iss.encode().hex(), acc.encode().hex(), raw.hex(), alg, digits, period, counter))
    q = lambda s: urllib.parse.quote(s, safe="-._~")
    uri = "otpauth://%s/%s%s?secret=%s" % ("hotp" if kind else "totp", (q(iss) + ":") if iss else "", q(acc), base64.b32encode(raw).decode().rstrip("="))
    if iss:
        uri += "&issuer=" + q(iss)
    uri += "&algorithm=%s&digits=%d" % (NAME[alg].upper(), digits)
    uri += "&counter=%d" % counter if kind else "&period=%d" % period
    want.append(uri + " ok")
# the builder writes the issuer as an empty string when there is none; the parse of the account
# with ':' in it is not part of the round trip contract, so those rows are skipped by the tool via
# the label rule: an account that contains ':' is not generated here.

r = subprocess.run([tool], input=("\n".join(lines) + "\n").encode(), capture_output=True)
got = r.stdout.decode(errors="replace").split("\n")
if r.returncode != 0 or len(got) < len(want):
    print("FAIL tool exit", r.returncode, "lines", len(got), "of", len(want), r.stderr[:200])
    sys.exit(1)

diffs = {"rfc_vectors": 0, "hotp_totp": 0, "base32_encode": 0, "base32_decode": 0, "uri_parse": 0, "uri_build": 0}
shown = 0
for i, w in enumerate(want):
    if got[i] == w:
        continue
    cat = ("rfc_vectors" if i < n_vectors else "hotp_totp" if i < n_hotp - 0 and i < n_vectors + cases else
           "base32_encode" if i < n_hotp + cases // 2 else "base32_decode" if i < n_b32 else
           "uri_parse" if i < n_uri else "uri_build")
    diffs[cat] += 1
    if shown < 6:
        print("DIFF %s: %r\n   got  %r\n   want %r" % (cat, lines[i][:100], got[i][:120], w[:120]))
        shown += 1

total = len(want)
print("checks %d (RFC vectors %d, hotp/totp %d, base32 %d, uri parse %d, uri build %d): differences %s" % (
    total, n_vectors, cases, n_b32 - n_hotp, n_uri - n_b32, total - n_uri, sum(diffs.values())))
sys.exit(1 if sum(diffs.values()) else 0)
