#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
# Generates lib/std/crypto/p256_comb.bin: the fixed-base table of P-256 for p256_public (4-bit comb).
# Layout: for window j = 0..63 and digit v = 1..15 one entry of 64 octets: x then y of v * 16^j * G, each in the
# Montgomery domain of p (times 2^256 mod p) as eight 32-bit limbs, little endian (the layout of big.Big.d).
# 64 * 15 * 64 = 61440 octets. Run: python3 tools/crypto/gen_p256_comb.py [--check]
import struct, sys, os
p = 0xffffffff00000001000000000000000000000000ffffffffffffffffffffffff
a = p - 3
gx = 0x6b17d1f2e12c4247f8bce6e563a440f277037d812deb33a0f4a13945d898c296
gy = 0x4fe342e2fe1a7f9b8ee7eb4a7c0f9e162bce33576b315ececbb6406837bf51f5
def add(P, Q):
    if P is None: return Q
    if Q is None: return P
    x1, y1 = P; x2, y2 = Q
    if x1 == x2:
        if (y1 + y2) % p == 0: return None
        l = (3 * x1 * x1 + a) * pow(2 * y1, -1, p) % p
    else:
        l = (y2 - y1) * pow(x2 - x1, -1, p) % p
    x3 = (l * l - x1 - x2) % p
    return (x3, (l * (x1 - x3) - y1) % p)
def limbs(v):
    return struct.pack('<8I', *[(v >> (32 * i)) & 0xffffffff for i in range(8)])
out = bytearray()
base = (gx, gy)
R = 1 << 256
for j in range(64):
    P = None
    for v in range(1, 16):
        P = add(P, base)
        out += limbs(P[0] * R % p) + limbs(P[1] * R % p)
    for _ in range(4):
        base = add(base, base)
path = os.path.join(os.path.dirname(__file__), '..', '..', 'lib', 'std', 'crypto', 'p256_comb.bin')
if '--check' in sys.argv:
    sys.exit(0 if open(path, 'rb').read() == bytes(out) else 1)
open(path, 'wb').write(bytes(out))
print(len(out))
