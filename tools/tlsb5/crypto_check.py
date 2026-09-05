#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-only
"""tools/tlsb5/crypto_check.py -- the primitives of round B5 against
somebody else's implementation.

Nothing here is a vector this repository wrote down. Every expected answer
comes from Python's `hashlib`, from the `cryptography` package (which is
OpenSSL underneath) or from the test vectors of the RFC that defines the
thing -- and the RFC vectors are in the file only where they pin something
a random comparison cannot, such as the clamping of X25519 or the empty
salt of HKDF.

THE COUNTER-CHECKS ARE COUNTED SEPARATELY. A verifier that returns true
for everything passes every positive test there is, so the tables below
contain cases that MUST fail: a signature with one bit flipped, an AEAD
tag that was tampered with, a public key that is not on the curve, an
exponent of 1. They are marked `False` and they are counted in their own
column, because a round that reports "412 of 412" without saying how many
of them were refusals has not measured what it thinks it has.
"""
import os
import subprocess
import sys
import hashlib
import hmac as pyhmac
import secrets

from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding, rsa, ec
from cryptography.hazmat.primitives.asymmetric.x25519 import (
    X25519PrivateKey, X25519PublicKey)
from cryptography.hazmat.primitives.ciphers.aead import (
    ChaCha20Poly1305, AESGCM)
from cryptography.hazmat.primitives.kdf.hkdf import HKDFExpand
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms

rng = secrets.SystemRandom()
CASES = []          # (job line, expected answer)
COUNTER = set()     # indices of the cases that must FAIL / must be refused


def add(job, want, counter=False):
    if counter:
        COUNTER.add(len(CASES))
    CASES.append((job, want))


def h(b):
    """Hex, and never an odd number of digits -- the job protocol reads
    octet pairs and an odd field is a malformed line, not a small number."""
    return b.hex() if b else "."


def hx(v, minbytes=1):
    t = "%x" % v
    if len(t) % 2:
        t = "0" + t
    while len(t) < minbytes * 2:
        t = "00" + t
    return t


# ------------------------------------------------------------ big numbers
def big_cases():
    for bits in (256, 512, 1024, 2048):
        for _ in range(3):
            m = rng.randrange(1 << (bits - 1), 1 << bits) | 1
            a = rng.randrange(0, 1 << (bits + 32))
            nb = (bits + 7) // 8
            add("BIGMOD %s %0*x" % (hx(a), nb * 2, m), "%0*x" % (nb * 2, a % m))
            b = rng.randrange(0, m)
            add("BIGMULMOD %s %s %0*x" % (hx(a), hx(b), nb * 2, m),
                "%0*x" % (nb * 2, (a * b) % m))
    # the exponentiation, at the two sizes a certificate really uses
    for bits in (1024, 2048):
        m = rng.randrange(1 << (bits - 1), 1 << bits) | 1
        a = rng.randrange(0, m)
        nb = (bits + 7) // 8
        add("BIGMODEXP %s %s %0*x" % (hx(a), hx(65537), nb * 2, m),
            "%0*x" % (nb * 2, pow(a, 65537, m)))


# ---------------------------------------------------------------- X25519
def x25519_cases():
    # RFC 7748 5.2 -- the two vectors that pin the clamping and the
    # masking of the top bit of u.
    add("X25519 a546e36bf0527c9d3b16154b82465edd62144c0ac1fc5a18506a2244ba449ac4"
        " e6db6867583030db3594c1a424b15f7c726624ec26b3353b10a903a6d0ab1c4c",
        "c3da55379de9c6908e94ea4df28d084f32eccf03491c71f754b4075577a28552")
    add("X25519 4b66e9d4d1b4673c5ad22691957d6af5c11b6421e0ea01d42ca4169e7918ba0d"
        " e5210f12786811d3f4b7959d0538ae2c31dbe7106fc03c3efc4cd549c715a493",
        "95cbde9476e8907d7aade45cb4b873f88b595a68799fa152e6f8f7647aac7957")
    # RFC 7748 6.1 -- Alice and Bob, and the same secret from both sides
    a_priv = bytes.fromhex(
        "77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a")
    b_priv = bytes.fromhex(
        "5dab087e624a8a4b79e17f8b83800ee66f3bb1292618b6fd1c2f8b27ff88e0eb")
    a = X25519PrivateKey.from_private_bytes(a_priv)
    b = X25519PrivateKey.from_private_bytes(b_priv)
    apub = a.public_key().public_bytes(serialization.Encoding.Raw,
                                       serialization.PublicFormat.Raw)
    bpub = b.public_key().public_bytes(serialization.Encoding.Raw,
                                       serialization.PublicFormat.Raw)
    add("X25519B " + h(a_priv), h(apub))
    add("X25519B " + h(b_priv), h(bpub))
    add("X25519 %s %s" % (h(a_priv), h(bpub)), h(a.exchange(b.public_key())))
    add("X25519 %s %s" % (h(b_priv), h(apub)), h(b.exchange(a.public_key())))
    # twenty random exchanges against OpenSSL
    for _ in range(20):
        p = X25519PrivateKey.generate()
        q = X25519PrivateKey.generate()
        pb = p.private_bytes(serialization.Encoding.Raw,
                             serialization.PrivateFormat.Raw,
                             serialization.NoEncryption())
        qpub = q.public_key().public_bytes(serialization.Encoding.Raw,
                                           serialization.PublicFormat.Raw)
        add("X25519 %s %s" % (h(pb), h(qpub)), h(p.exchange(q.public_key())))
    # COUNTER-CHECK: a point of small order gives the all-zero secret, and
    # RFC 7748 6.1 says that must be reported as a failure.
    zero_u = "00" * 32
    add("X25519 %s %s" % ("01" * 32, zero_u), "ERR", counter=True)


# ------------------------------------------------- ChaCha20 and Poly1305
def chacha_cases():
    # RFC 8439 2.4.2 -- the block that pins the constants and the counter
    key = bytes(range(32))
    nonce = bytes.fromhex("000000090000004a00000000")
    pt = (b"Ladies and Gentlemen of the class of '99: If I could offer you "
          b"only one tip for the future, sunscreen would be it.")
    c = Cipher(algorithms.ChaCha20(key, (1).to_bytes(4, "little") + nonce),
               None).encryptor()
    add("CHACHA20 %s %s 01 %s" % (h(key), h(nonce), h(pt)),
        h(c.update(pt)))
    # RFC 8439 2.5.2 -- the Poly1305 vector
    pkey = bytes.fromhex(
        "85d6be7857556d337f4452fe42d506a80103808afb0db2fd4abff6af4149f51b")
    msg = b"Cryptographic Forum Research Group"
    add("POLY1305 %s %s" % (h(pkey), h(msg)),
        "a8061dc1305136c6c22b8baf0c0127a9")
    # thirty random AEAD seals against OpenSSL
    for _ in range(30):
        k = secrets.token_bytes(32)
        n = secrets.token_bytes(12)
        aad = secrets.token_bytes(rng.randrange(0, 40))
        p = secrets.token_bytes(rng.randrange(0, 300))
        want = ChaCha20Poly1305(k).encrypt(n, p, aad)
        add("AEADCC %s %s %s %s" % (h(k), h(n), h(aad), h(p)),
            h(want))
        add("AEADCCOPEN %s %s %s %s" % (h(k), h(n), h(aad), h(want)),
            h(p))
    # COUNTER-CHECKS: a flipped bit anywhere must make `open` fail.
    k = secrets.token_bytes(32)
    n = secrets.token_bytes(12)
    p = b"the message"
    ct = bytearray(ChaCha20Poly1305(k).encrypt(n, p, b""))
    for pos in (0, len(ct) - 1, len(ct) - 16):
        bad = bytearray(ct)
        bad[pos] ^= 0x01
        add("AEADCCOPEN %s %s %s %s" % (h(k), h(n), ".", h(bytes(bad))),
            "ERR", counter=True)


# ----------------------------------------------------------- AES-128-GCM
def gcm_cases():
    for _ in range(25):
        k = secrets.token_bytes(16)
        n = secrets.token_bytes(12)
        aad = secrets.token_bytes(rng.randrange(0, 40))
        p = secrets.token_bytes(rng.randrange(0, 300))
        want = AESGCM(k).encrypt(n, p, aad)
        add("AEADGCM %s %s %s %s" % (h(k), h(n), h(aad), h(p)),
            h(want))
        add("AEADGCMOPEN %s %s %s %s" % (h(k), h(n), h(aad), h(want)),
            h(p))
    k = secrets.token_bytes(16)
    n = secrets.token_bytes(12)
    ct = bytearray(AESGCM(k).encrypt(n, b"the message", b"head"))
    for pos in (0, len(ct) - 1):
        bad = bytearray(ct)
        bad[pos] ^= 0x80
        add("AEADGCMOPEN %s %s %s %s" % (h(k), h(n), h(b"head"),
                                          h(bytes(bad))), "ERR", counter=True)
    # the aad matters: the right ciphertext under the wrong aad must fail
    good = AESGCM(k).encrypt(n, b"the message", b"head")
    add("AEADGCMOPEN %s %s %s %s" % (h(k), h(n), h(b"heaD"), h(good)),
        "ERR", counter=True)


# ------------------------------------------------------ SHA-384/512, HKDF
def hash_cases():
    for n in list(range(0, 200)) + [255, 256, 257, 1000, 4096]:
        m = secrets.token_bytes(n)
        add("SHA512 " + h(m), hashlib.sha512(m).hexdigest())
        add("SHA384 " + h(m), hashlib.sha384(m).hexdigest())
    # RFC 5869 appendix A, all three SHA-256 cases including the empty salt
    a1 = ("0b" * 22, "000102030405060708090a0b0c", "f0f1f2f3f4f5f6f7f8f9", 42)
    a2 = ("".join("%02x" % i for i in range(80)),
          "".join("%02x" % i for i in range(0x60, 0xb0)),
          "".join("%02x" % i for i in range(0xb0, 0x100)), 82)
    a3 = ("0b" * 22, "", "", 42)
    for ikm, salt, info, ln in (a1, a2, a3):
        ikm_b = bytes.fromhex(ikm)
        salt_b = bytes.fromhex(salt)
        info_b = bytes.fromhex(info)
        prk = pyhmac.new(salt_b or b"\0" * 32, ikm_b, hashlib.sha256).digest()
        add("HKDFEXT %s %s" % (salt or ".", ikm), h(prk))
        okm = HKDFExpand(algorithm=hashes.SHA256(), length=ln,
                         info=info_b).derive(prk)
        add("HKDFEXP %s %s %02x" % (h(prk), info or ".", ln), h(okm))


# --------------------------------------------------------------- RSA, PSS
def rsa_cases():
    for bits in (1024, 2048, 3072):
        key = rsa.generate_private_key(public_exponent=65537, key_size=bits)
        pub = key.public_key().public_numbers()
        n = "%0*x" % (bits // 4, pub.n)
        e = hx(pub.e)
        msg = secrets.token_bytes(100)
        dg = hashlib.sha256(msg).digest()
        sig = key.sign(msg, padding.PKCS1v15(), hashes.SHA256())
        add("RSAPKCS %s %s %s %s" % (n, e, h(sig), h(dg)), "OK")
        # COUNTER-CHECK: one bit of the signature
        bad = bytearray(sig)
        bad[10] ^= 0x01
        add("RSAPKCS %s %s %s %s" % (n, e, h(bytes(bad)), h(dg)), "BAD",
            counter=True)
        # COUNTER-CHECK: the right signature over a different hash
        other = hashlib.sha256(msg + b"!").digest()
        add("RSAPKCS %s %s %s %s" % (n, e, h(sig), h(other)), "BAD",
            counter=True)
        pss = key.sign(msg, padding.PSS(mgf=padding.MGF1(hashes.SHA256()),
                                        salt_length=32), hashes.SHA256())
        add("RSAPSS %s %s %s %s" % (n, e, h(pss), h(dg)), "OK")
        bad = bytearray(pss)
        bad[-2] ^= 0x40
        add("RSAPSS %s %s %s %s" % (n, e, h(bytes(bad)), h(dg)), "BAD",
            counter=True)
        # COUNTER-CHECK: a PSS signature is not a PKCS#1 one
        add("RSAPKCS %s %s %s %s" % (n, e, h(pss), h(dg)), "BAD",
            counter=True)
        # COUNTER-CHECK: an exponent of 1 must be refused outright
        add("RSAPKCS %s %s %s %s" % (n, "01", h(sig), h(dg)), "BAD",
            counter=True)


# ------------------------------------------------------------ ECDSA P-256
def p256_cases():
    for _ in range(12):
        key = ec.generate_private_key(ec.SECP256R1())
        nums = key.public_key().public_numbers()
        qx = "%064x" % nums.x
        qy = "%064x" % nums.y
        msg = secrets.token_bytes(60)
        dg = hashlib.sha256(msg).digest()
        der = key.sign(msg, ec.ECDSA(hashes.SHA256()))
        from cryptography.hazmat.primitives.asymmetric.utils import (
            decode_dss_signature)
        r, s = decode_dss_signature(der)
        add("P256 %s %s %064x %064x %s" % (qx, qy, r, s, h(dg)), "OK")
        # COUNTER-CHECK: r and s swapped
        add("P256 %s %s %064x %064x %s" % (qx, qy, s, r, h(dg)), "BAD",
            counter=True)
        # COUNTER-CHECK: another message
        add("P256 %s %s %064x %064x %s" % (qx, qy, r, s,
                                            h(hashlib.sha256(msg + b"x")
                                              .digest())), "BAD",
            counter=True)
    # COUNTER-CHECK: s = 0 and r = 0 are not signatures
    key = ec.generate_private_key(ec.SECP256R1())
    nums = key.public_key().public_numbers()
    qx = "%064x" % nums.x
    qy = "%064x" % nums.y
    dg = hashlib.sha256(b"x").digest()
    add("P256 %s %s %064x %064x %s" % (qx, qy, 0, 1, h(dg)), "BAD",
        counter=True)
    add("P256 %s %s %064x %064x %s" % (qx, qy, 1, 0, h(dg)), "BAD",
        counter=True)
    # COUNTER-CHECK: a public key that is NOT on the curve
    der = key.sign(b"x", ec.ECDSA(hashes.SHA256()))
    from cryptography.hazmat.primitives.asymmetric.utils import (
        decode_dss_signature)
    r, s = decode_dss_signature(der)
    add("P256 %s %s %064x %064x %s" % (qx, "%064x" % (nums.y ^ 1), r, s,
                                        h(dg)), "BAD", counter=True)


def main():
    binary = sys.argv[1]
    big_cases()
    x25519_cases()
    chacha_cases()
    gcm_cases()
    hash_cases()
    rsa_cases()
    p256_cases()
    jobs = "\n".join(c[0] for c in CASES) + "\n"
    r = subprocess.run([binary], input=jobs.encode(), capture_output=True)
    got = r.stdout.decode().split("\n")
    ok = 0
    ok_counter = 0
    bad = []
    for i, (job, want) in enumerate(CASES):
        g = got[i].strip() if i < len(got) else "<missing>"
        if g == want:
            ok += 1
            if i in COUNTER:
                ok_counter += 1
        else:
            bad.append((job.split()[0], job[:70], want[:40], g[:40]))
    print("   %d / %d primitive cases, of them %d / %d counter-checks"
          % (ok, len(CASES), ok_counter, len(COUNTER)))
    for kind, job, want, g in bad[:20]:
        print("  FAIL  %-12s want %-40s got %s" % (kind, want, g))
        print("        %s" % job)
    if bad:
        print("CRYPTO FAILED: %d" % len(bad))
        return 1
    print("CRYPTO OK: %d / %d, counter-checks %d"
          % (ok, len(CASES), ok_counter))
    return 0


if __name__ == "__main__":
    sys.exit(main())
