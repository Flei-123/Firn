#!/usr/bin/env python3
"""Generates tests/data/keyring-vectors.json for lib/std/crypto/key*.fi (docs/ORBIT-KEYS-SPEC.md in FirnChat).

Independent oracles only: Python `cryptography` (HKDF, ChaCha20-Poly1305, X25519, Ed25519), hashlib,
base64 and the C reference tool /usr/bin/argon2. Deterministic: fixed byte patterns, no clock, no randomness.

    python3 -I tools/keyring/gen_vectors.py > tests/data/keyring-vectors.json
"""
import base64
import hashlib
import hmac
import json
import struct
import subprocess

from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.asymmetric.x25519 import X25519PrivateKey, X25519PublicKey
from cryptography.hazmat.primitives.ciphers.aead import ChaCha20Poly1305
from cryptography.hazmat.primitives.kdf.hkdf import HKDF, HKDFExpand
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

ZERO12 = bytes(12)


def pat(n, seed):
    return bytes((i * 7 + seed) & 255 for i in range(n))


def u32(v):
    return struct.pack(">I", v)


def lp8(b):
    assert len(b) <= 255
    return bytes([len(b)]) + b


def hkdf(salt, ikm, info, n=32):
    return HKDF(hashes.SHA256(), n, salt, info).derive(ikm)


def prk_of(amk):
    return hmac.new(b"ORBIT-AMK-v1", amk, hashlib.sha256).digest()


def expand(prk, info, n=32):
    return HKDFExpand(hashes.SHA256(), n, info).derive(prk)


def subkey(amk, name):
    return expand(prk_of(amk), b"orbit/sub/v1/" + name.encode())


def kid(amk):
    return expand(prk_of(amk), b"orbit/kid/v1", 8)


def aead(key, aad, plain):
    return ChaCha20Poly1305(key).encrypt(ZERO12, plain, aad)


def x_pub(sec):
    return X25519PrivateKey.from_private_bytes(sec).public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)


def x_shared(sec, pub):
    return X25519PrivateKey.from_private_bytes(sec).exchange(X25519PublicKey.from_public_bytes(pub))


def ed_pub(seed):
    return Ed25519PrivateKey.from_private_bytes(seed).public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)


def ed_sign(seed, ctx, msg):
    return Ed25519PrivateKey.from_private_bytes(seed).sign(b"ORBIT-SIG1" + lp8(ctx.encode()) + msg)


def b32(raw):
    return base64.b32encode(raw).decode().rstrip("=")


def group4(s):
    return "-".join(s[i:i + 4] for i in range(0, len(s), 4))


def argon2id(pw, salt, t, kib, p):
    r = subprocess.run(["/usr/bin/argon2", salt.decode(), "-id", "-t", str(t), "-k", str(kib),
                        "-p", str(p), "-l", "32", "-r"], input=pw, capture_output=True)
    assert r.returncode == 0, r.stderr
    return bytes.fromhex(r.stdout.decode().strip())


def record(amk, epoch, acct, cls, oid, objver, salt, plain):
    aad = (b"ORBIT-REC1" + b"\x01" + u32(epoch) + lp8(acct.encode()) + lp8(cls.encode())
           + lp8(oid.encode()) + u32(objver))
    key = hkdf(salt, subkey(amk, cls), b"orbit/rec/v1")
    return b"\x01" + u32(epoch) + salt + aead(key, aad, plain)


def wrap_dev(amk, epoch, acct, eph_sec, dev_pub):
    eph_pub = x_pub(eph_sec)
    key = hkdf(eph_pub + dev_pub, x_shared(eph_sec, dev_pub), b"orbit/wrap-dev/v1")
    aad = b"ORBIT-WDEV1" + b"\x01" + u32(epoch) + lp8(acct.encode()) + dev_pub
    return b"\x01" + u32(epoch) + eph_pub + aead(key, aad, amk)


def wrap_pw(amk, epoch, acct, pw, salt, m, t, p):
    raw = argon2id(pw, salt, t, m, p)
    key = hkdf(b"ORBIT-PW-v1", raw, b"orbit/wrap-pw/v1")
    params = b"\x01" + u32(m) + bytes([t, p]) + salt
    aad = b"ORBIT-WPW1" + b"\x01" + u32(epoch) + lp8(acct.encode()) + params
    return b"\x01" + u32(epoch) + params + aead(key, aad, amk)


def wrap_rk(amk, epoch, acct, rk, salt):
    key = hkdf(salt, rk, b"orbit/wrap-rk/v1")
    aad = b"ORBIT-WRK1" + b"\x01" + u32(epoch) + lp8(acct.encode())
    return b"\x01" + u32(epoch) + salt + aead(key, aad, amk)


def rk_text(rk):
    check = hashlib.sha256(b"ORBIT-RK-CHECK1" + rk).digest()[:3]
    return group4(b32(rk + check))


def fp_text(data):
    return group4(b32(hashlib.sha256(b"ORBIT-FP1" + data).digest()[:10]))


def local_store(pw, salt, m, t, p, acct, plain):
    raw = argon2id(pw, salt, t, m, p)
    key = hkdf(b"ORBIT-LOCAL-v1", raw, b"orbit/local/v1")
    params = b"\x01" + u32(m) + bytes([t, p]) + salt
    aad = b"ORBIT-LOCAL1" + b"\x01" + lp8(acct.encode()) + params
    return b"\x01" + params + aead(key, aad, plain)


def main():
    out = {}
    amks = [pat(32, 1), pat(32, 77)]
    names = ["contacts", "calendar", "tasks", "mail", "search-index", "vault", "drive", "notes",
             "identity", "amk-chain", "sharing", "a", "x-1-y"]
    out["subkeys"] = [{"id": f"amk{i}", "amk": a.hex(), "kid": kid(a).hex(),
                       "names": {n: subkey(a, n).hex() for n in names}} for i, a in enumerate(amks)]

    recs = []
    cases = [("contacts", 1, "0123456789abcdef", "c1", 1, 0), ("calendar", 3, "0123456789abcdef", "ev-0042", 7, 5),
             ("calendar", 1, "fedcba9876543210", "x" * 255, 4294967295, 1), ("vault", 2, "0123456789abcdef", "o", 0, 200),
             ("notes", 9, "a", "n", 1, 1000)]
    for i, (cls, ep, acct, oid, ver, n) in enumerate(cases):
        a = amks[i % 2]
        salt = pat(16, 40 + i)
        plain = pat(n, i)
        recs.append({"id": f"rec{i}", "amk": a.hex(), "epoch": ep, "acct": acct, "class": cls, "oid": oid,
                     "objver": ver, "salt": salt.hex(), "plain": plain.hex(),
                     "rec": record(a, ep, acct, cls, oid, ver, salt, plain).hex()})
    out["records"] = recs

    wd = []
    for i in range(3):
        dev_sec = pat(32, 100 + i)
        wd.append({"id": f"dev{i}", "amk": amks[i % 2].hex(), "epoch": i + 1, "acct": "0123456789abcdef",
                   "eph_sec": pat(32, 120 + i).hex(), "dev_sec": dev_sec.hex(), "dev_pub": x_pub(dev_sec).hex(),
                   "blob": wrap_dev(amks[i % 2], i + 1, "0123456789abcdef", pat(32, 120 + i), x_pub(dev_sec)).hex()})
    out["wrap_dev"] = wd

    wp = []
    pcases = [("correct horse battery".encode(), 8192, 1, 1, 1), ("pässwörd-ünïcode 123".encode(), 8192, 2, 1, 2),
              (b"x" * 40 + b"0123456789", 16384, 1, 1, 3),
              ("default parameters ok".encode(), 65536, 3, 1, 4)]
    for i, (pw, m, t, p, ep) in enumerate(pcases):
        salt = b"orbit-test-salt" + bytes([48 + i])
        wp.append({"id": f"pw{i}", "amk": amks[i % 2].hex(), "epoch": ep, "acct": "0123456789abcdef", "pw": pw.hex(),
                   "salt": salt.hex(), "m": m, "t": t, "p": p,
                   "blob": wrap_pw(amks[i % 2], ep, "0123456789abcdef", pw, salt, m, t, p).hex()})
    out["wrap_pw"] = wp

    wr = []
    for i in range(2):
        rk = pat(32, 150 + i)
        salt = pat(16, 160 + i)
        wr.append({"id": f"rk{i}", "amk": amks[i].hex(), "epoch": i + 1, "acct": "0123456789abcdef", "rk": rk.hex(),
                   "salt": salt.hex(), "text": rk_text(rk),
                   "blob": wrap_rk(amks[i], i + 1, "0123456789abcdef", rk, salt).hex()})
    out["wrap_rk"] = wr
    out["rk_text"] = [{"rk": pat(32, s).hex(), "text": rk_text(pat(32, s))} for s in (0, 1, 2, 255)]
    out["rk_text"].append({"rk": bytes(32).hex(), "text": rk_text(bytes(32))})

    s_seed = pat(32, 11)
    x_sec = pat(32, 12)
    bundle = b"\x01" + x_pub(x_sec) + ed_pub(s_seed) + u32(1788000000)
    acct = "0123456789abcdef"
    sig = ed_sign(s_seed, "identity", lp8(acct.encode()) + bundle)
    new_seed = pat(32, 13)
    out["ident"] = {"acct": acct, "s_seed": s_seed.hex(), "s_pub": ed_pub(s_seed).hex(), "x_sec": x_sec.hex(),
                    "x_pub": x_pub(x_sec).hex(), "created": 1788000000, "bundle": bundle.hex(), "sig": sig.hex(),
                    "fp": fp_text(bundle),
                    "new_seed": new_seed.hex(), "new_pub": ed_pub(new_seed).hex(),
                    "rot_old": ed_sign(s_seed, "rotation", lp8(acct.encode()) + ed_pub(s_seed) + ed_pub(new_seed)).hex(),
                    "rot_new": ed_sign(new_seed, "rotation", lp8(acct.encode()) + ed_pub(s_seed) + ed_pub(new_seed)).hex()}
    out["fp"] = [{"data": pat(n, n).hex(), "text": fp_text(pat(n, n))} for n in (0, 1, 32, 69, 100)]

    lc = []
    for i, (pw, m, t) in enumerate([("local pass phrase".encode(), 8192, 1), ("ünï 2nd pass".encode(), 16384, 2)]):
        salt = b"orbit-local-salt" if i == 0 else b"orbit-local-sal2"
        plain = pat(64, 90 + i)
        lc.append({"id": f"local{i}", "acct": "0123456789abcdef", "pw": pw.hex(), "salt": salt.hex(), "m": m, "t": t,
                   "p": 1, "plain": plain.hex(),
                   "blob": local_store(pw, salt, m, t, 1, "0123456789abcdef", plain).hex()})
    out["local"] = lc
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
