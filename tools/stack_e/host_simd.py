# tools/stack_e/host_simd.py -- G6 SIMD intrinsics from a Firn .so against numpy (random data, 2000 rounds).
import ctypes as C, sys
import numpy as np
l = C.CDLL(sys.argv[1])
l.se_mask8.restype = C.c_uint32; l.se_maskf.restype = C.c_uint32; l.se_dot4.restype = C.c_float
for n in ("se_sqrt4", "se_packs32", "se_packus16", "se_mulhi16", "se_mask8", "se_maskf", "se_dot4"):
    getattr(l, n).argtypes = [C.c_uint64] * (3 if n in ("se_packs32", "se_packus16", "se_mulhi16") else 2 if n in ("se_sqrt4", "se_dot4") else 1)
rng = np.random.default_rng(7)
bad = 0
def p(x): return x.ctypes.data
for _ in range(2000):
    f = rng.uniform(0, 1e4, 4).astype(np.float32); o = np.zeros(4, np.float32)
    l.se_sqrt4(p(o), p(f)); bad += not np.array_equal(o, np.sqrt(f))
    a32 = rng.integers(-200000, 200000, 4).astype(np.int32); b32 = rng.integers(-200000, 200000, 4).astype(np.int32)
    o16 = np.zeros(8, np.int16); l.se_packs32(p(o16), p(a32), p(b32))
    bad += not np.array_equal(o16, np.clip(np.concatenate([a32, b32]), -32768, 32767).astype(np.int16))
    a16 = rng.integers(-32768, 32768, 8).astype(np.int16); b16 = rng.integers(-32768, 32768, 8).astype(np.int16)
    o8 = np.zeros(16, np.uint8); l.se_packus16(p(o8), p(a16), p(b16))
    bad += not np.array_equal(o8, np.clip(np.concatenate([a16, b16]).astype(np.int32), 0, 255).astype(np.uint8))
    oh = np.zeros(8, np.int16); l.se_mulhi16(p(oh), p(a16), p(b16))
    bad += not np.array_equal(oh, ((a16.astype(np.int32) * b16.astype(np.int32)) >> 16).astype(np.int16))
    by = rng.integers(0, 256, 16).astype(np.uint8)
    bad += l.se_mask8(p(by)) != int(sum(1 << i for i in range(16) if by[i] >= 128))
    u = rng.integers(0, 2**32, 4, dtype=np.uint64).astype(np.uint32)
    bad += l.se_maskf(p(u)) != int(sum(1 << i for i in range(4) if u[i] >> 31))
    x = rng.uniform(-100, 100, 4).astype(np.float32); y = rng.uniform(-100, 100, 4).astype(np.float32)
    prod = x * y; want = np.float32(np.float32(prod[0] + prod[1]) + np.float32(prod[2] + prod[3]))
    bad += l.se_dot4(p(x), p(y)) != want
print("ok  host_simd.py (numpy)" if not bad else "FAIL host_simd.py: %d mismatches" % bad)
sys.exit(1 if bad else 0)
