#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# tools/tls/p256_check.py -- lib/std/crypto/p256.fi (ECDSA signing) against
# Python's `cryptography` (OpenSSL):
#   * RFC 6979 A.2.5 (P-256, SHA-256): the deterministic signatures, exact
#   * public keys of random scalars, and of 1, 2, n-1
#   * k * P for random points, and k = 0 / k = n giving the point at infinity
#   * random keys and hashes: every signature verifies with OpenSSL, with and
#     without extra entropy; counter-checks: a flipped bit MUST fail
#   * a crude timing look: scalars with 1 bit set vs. 255 bits set
import os, random, subprocess, sys, time, hashlib
from cryptography.hazmat.primitives.asymmetric import ec, utils
from cryptography.hazmat.primitives import hashes
from cryptography.exceptions import InvalidSignature

N = 0xFFFFFFFF00000000FFFFFFFFFFFFFFFFBCE6FAADA7179E84F3B9CAC2FC632551
BIN = sys.argv[1]
ok = 0
total = 0
counter = 0


def run(*args):
    r = subprocess.run([BIN] + list(args), capture_output=True, text=True, timeout=60)
    if r.returncode != 0:
        raise RuntimeError("exit %d for %s" % (r.returncode, args[0]))
    return r.stdout.strip()


def check(name, cond):
    global ok, total
    total += 1
    if cond:
        ok += 1
    else:
        print("  FAIL", name)


def h32(v):
    return "%064x" % v


# 1. RFC 6979 A.2.5
x = 0xC9AFA9D845BA75166B5C215767B1D6934E50C3DB36E89B127B8A622B120F6721
for msg, r_want, s_want in [
    (b"sample", 0xEFD48B2AACB6A8FD1140DD9CD45E81D69D2C877B56AAF991C34D0EA84EAF3716,
     0xF7CB1C942D657C41D436C7A1B6E29F65F3E900DBB9AFF4064DC4AB2F843ACDA8),
    (b"test", 0xF1ABB023518351CD71D881567B1EA663ED3EFCF6C5132B354F28D3B0B7D38367,
     0x019F4113742A2B14BD25926B49C649155F267E60D3814B4C0CC84250E46F0083),
]:
    out = run("sign", h32(x), hashlib.sha256(msg).hexdigest()).split()
    check("rfc6979 " + msg.decode(), int(out[0], 16) == r_want and int(out[1], 16) == s_want)

# 2. public keys
for d in [1, 2, 3, N - 1, N - 2] + [random.randrange(1, N) for _ in range(40)]:
    pub = ec.derive_private_key(d, ec.SECP256R1()).public_key().public_numbers()
    got = run("pub", h32(d))
    check("pub %x" % d, got == "04" + h32(pub.x) + h32(pub.y))

# 3. k * P
for _ in range(30):
    pk = ec.generate_private_key(ec.SECP256R1()).public_key().public_numbers()
    k = random.randrange(1, N)
    want = ec.derive_private_key(k, ec.SECP256R1())  # k*G only; use ECDH identity instead
    # k*P via the relation (k*a)*G where P = a*G
    a = random.randrange(1, N)
    P = ec.derive_private_key(a, ec.SECP256R1()).public_key().public_numbers()
    Q = ec.derive_private_key((k * a) % N, ec.SECP256R1()).public_key().public_numbers()
    got = run("mult", h32(k), h32(P.x), h32(P.y)).split()
    check("mult", got == [h32(Q.x), h32(Q.y)])
P = ec.derive_private_key(7, ec.SECP256R1()).public_key().public_numbers()
check("0*P = O", run("mult", h32(0), h32(P.x), h32(P.y)) == "inf")
check("n*P = O", run("mult", h32(N), h32(P.x), h32(P.y)) == "inf")

# 4. signatures verified by OpenSSL, counter-checks
for i in range(60):
    key = ec.generate_private_key(ec.SECP256R1())
    d = key.private_numbers().private_value
    msg = os.urandom(random.randrange(0, 200))
    hh = hashlib.sha256(msg).digest()
    args = ["sign", h32(d), hh.hex()]
    if i % 2:
        args.append(os.urandom(32).hex())
    r, s, der = run(*args).split()
    derb = bytes.fromhex(der)
    check("der", utils.decode_dss_signature(derb) == (int(r, 16), int(s, 16)))
    try:
        key.public_key().verify(derb, hh, ec.ECDSA(utils.Prehashed(hashes.SHA256())))
        check("verify", True)
    except InvalidSignature:
        check("verify", False)
    bad = bytearray(hh)
    bad[random.randrange(32)] ^= 1 << random.randrange(8)
    try:
        key.public_key().verify(derb, bytes(bad), ec.ECDSA(utils.Prehashed(hashes.SHA256())))
        check("counter-check (must fail)", False)
    except InvalidSignature:
        check("counter-check (must fail)", True)
        counter += 1

# 5. timing: 1-bit vs 255-bit scalars
def t_of(d):
    t0 = time.perf_counter()
    for _ in range(10):
        run("pub", h32(d))
    return (time.perf_counter() - t0) / 10
lo = t_of(1 << 200)
hi = t_of((1 << 255) - 1 - (1 << 3))
print("   timing: scalar with 1 bit %.2f ms, with 254 bits %.2f ms (process start included)"
      % (lo * 1000, hi * 1000))
print("   %d / %d P-256 cases, of them %d counter-checks" % (ok, total, counter))
print("P256 OK: %d / %d" % (ok, total) if ok == total else "P256 FAILED: %d / %d" % (ok, total))
sys.exit(0 if ok == total else 1)
