# tools/stack_e/host.py -- Python ctypes side of the way-E probe (+ numpy cross-check).
import ctypes as C, sys, numpy as np

lib = C.CDLL(sys.argv[1])
fails = 0
checks = 0
def check(c, name):
    global fails, checks
    checks += 1
    if not c:
        fails += 1
        print("FAIL", name)

class Vtx(C.Structure):
    _fields_ = [(n, C.c_float) for n in "xyzw"]
class V2(C.Structure):
    _fields_ = [("x", C.c_float), ("y", C.c_float)]

lib.se_abi.restype = C.c_uint64
lib.se_add_i32.argtypes = [C.c_int32, C.c_int32]; lib.se_add_i32.restype = C.c_int32
lib.se_axpy_f32.argtypes = [C.c_float] * 3; lib.se_axpy_f32.restype = C.c_float
lib.se_len_f64.argtypes = [C.c_double] * 2; lib.se_len_f64.restype = C.c_double
lib.se_vtx_scale.argtypes = [C.POINTER(Vtx), C.c_float]
lib.se_vtx_dot.argtypes = [C.POINTER(Vtx)] * 2; lib.se_vtx_dot.restype = C.c_float
lib.se_v2_add.argtypes = [V2, V2]; lib.se_v2_add.restype = V2
CB = C.CFUNCTYPE(C.c_int64, C.c_int64)
lib.se_call_cb.argtypes = [CB, C.c_int64]; lib.se_call_cb.restype = C.c_int64
lib.se_count_inc.restype = C.c_uint64
P = C.c_void_p
lib.se_fma4_f32.argtypes = [P, P, P, P, C.c_size_t]
lib.se_mix_span.argtypes = [P, C.c_size_t, C.c_uint32, C.c_uint32]
lib.se_blend_span.argtypes = [P, C.c_size_t, C.c_uint32, C.c_uint32]

check(lib.se_abi() == 1, "abi")
check(lib.se_add_i32(-5, 12) == 7, "add_i32")
check(lib.se_axpy_f32(2.0, 3.5, 1.0) == 8.0, "axpy_f32")
check(lib.se_len_f64(3.0, 4.0) == 25.0, "len_f64")
a = Vtx(1, 2, 3, 4); b = Vtx(0.5, 1, 2, 3)
check(lib.se_vtx_dot(C.byref(a), C.byref(b)) == 20.5, "vtx_dot")
lib.se_vtx_scale(C.byref(a), 2.0)
check((a.x, a.y, a.z, a.w) == (2, 4, 6, 8), "vtx_scale")
r = lib.se_v2_add(V2(1, 2), V2(10, 20))
check((r.x, r.y) == (11, 22), "v2_add")
seen = []
def py_cb(x):
    seen.append(x); return x * x
cb = CB(py_cb)
check(lib.se_call_cb(cb, 10) == 285 and seen == list(range(10)), "callback (ctypes)")
check(lib.se_count_inc() == 1 and lib.se_count_inc() == 2, "global counter")

rng = np.random.default_rng(3)
x, y, z = (rng.standard_normal(1000).astype(np.float32) for _ in range(3))
o = np.empty_like(x)
lib.se_fma4_f32(o.ctypes.data, x.ctypes.data, y.ctypes.data, z.ctypes.data, 1000)
check(np.array_equal(o, (x * y) + z), "fma4_f32 vs numpy")

def mix_ref(px, rgb, f):
    px = px.astype(np.uint32); out = np.zeros_like(px)
    for c in range(4):
        cc = (px >> (8 * c)) & 255
        k = 255 if c == 3 else (rgb >> (8 * c)) & 255
        out |= (((cc * (256 - f) + k * f) >> 8) & 255) << (8 * c)
    return out
def blend_ref(px, rgb, al):
    px = px.astype(np.uint32); out = np.zeros_like(px)
    for c in range(4):
        cc = (px >> (8 * c)) & 255
        k = 255 if c == 3 else (rgb >> (8 * c)) & 255
        out |= (((cc * (255 - al) + k * al) // 255) & 255) << (8 * c)
    return out
ok1 = ok2 = True
for f in range(0, 257, 8):
    px = rng.integers(0, 2**32, 1001, dtype=np.uint32); w = px.copy()
    lib.se_mix_span(w.ctypes.data, 1001, 0x00AA5533, f)
    ok1 &= np.array_equal(w, mix_ref(px, 0x00AA5533, f))
for al in range(0, 256, 9):
    px = rng.integers(0, 2**32, 1001, dtype=np.uint32); w = px.copy()
    lib.se_blend_span(w.ctypes.data, 1001, 0x00AA5533, al)
    ok2 &= np.array_equal(w, blend_ref(px, 0x00AA5533, al))
check(ok1, "mix_span vs numpy"); check(ok2, "blend_span vs numpy")
print(f"Python host: {checks} checks, {fails} failures")
sys.exit(1 if fails else 0)
