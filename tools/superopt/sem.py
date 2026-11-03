import numpy as np

M64 = (1 << 64) - 1


def mask(w):
    return (1 << w) - 1


class NumpyBackend:
    def const(self, v):
        return np.uint64(v & M64)

    def lshr(self, a, c):
        return a >> np.uint64(c) if isinstance(c, int) else a >> c

    def shl(self, a, c):
        return a << np.uint64(c) if isinstance(c, int) else a << c

    def ashr64(self, a, c):
        c = np.uint64(c) if isinstance(c, int) else c
        return (a.view(np.int64) >> c.astype(np.int64)).view(np.uint64)

    def is_concrete(self):
        return True


class Z3Backend:
    def __init__(self):
        import z3
        self.z3 = z3

    def const(self, v):
        return self.z3.BitVecVal(v & M64, 64)

    def lshr(self, a, c):
        return self.z3.LShR(a, self._c(c))

    def shl(self, a, c):
        return a << self._c(c)

    def ashr64(self, a, c):
        return a >> self._c(c)

    def _c(self, c):
        return self.const(c) if isinstance(c, int) else c

    def is_concrete(self):
        return False


def sext(B, x, w):
    if w == 64:
        return x
    s = B.const(1 << (w - 1))
    return ((x & B.const(mask(w))) ^ s) - s


def read(B, st, op):
    kind = op[0]
    if kind == "i":
        return B.const(op[1])
    _, v, w = op
    return st[v] if w == 64 else st[v] & B.const(mask(w))


def write(B, st, op, val):
    _, v, w = op
    if w == 64:
        st[v] = val
    elif w == 32:
        st[v] = val & B.const(mask(32))
    else:
        m = mask(w)
        st[v] = (st[v] & B.const(M64 ^ m)) | (val & B.const(m))


def shift_count(B, st, op, w):
    lim = 63 if w == 64 else 31
    if op[0] == "i":
        return op[1] & lim
    return read(B, st, op) & B.const(lim)


def rotate(B, a, c, w, left):
    c %= w
    if c == 0:
        return a
    m = B.const(mask(w))
    if not left:
        c = w - c
    return (B.shl(a, c) | B.lshr(a, w - c)) & m


def addr(B, st, terms):
    acc = B.const(0)
    for t in terms:
        if t[0] == "d":
            acc = acc + B.const(t[1])
        else:
            _, v, scale, w = t
            acc = acc + read(B, st, ("r", v, w)) * B.const(scale)
    return acc


def step(B, st, insn):
    mn, ops = insn
    d = ops[0] if ops else None
    w = d[2] if d and d[0] == "r" else 64
    m = B.const(mask(w))
    if mn == "mov":
        write(B, st, d, read(B, st, ops[1]) & m)
    elif mn == "movzx":
        write(B, st, d, read(B, st, ops[1]))
    elif mn in ("movsx", "movsxd"):
        write(B, st, d, sext(B, read(B, st, ops[1]), ops[1][2]) & m)
    elif mn == "lea":
        write(B, st, d, addr(B, st, ops[1][1]) & m)
    elif mn in ("add", "sub", "and", "or", "xor"):
        a, b = read(B, st, d), read(B, st, ops[1]) & m
        r = {"add": lambda: a + b, "sub": lambda: a - b, "and": lambda: a & b,
             "or": lambda: a | b, "xor": lambda: a ^ b}[mn]()
        write(B, st, d, r & m)
    elif mn == "imul":
        if len(ops) == 3:
            r = read(B, st, ops[1]) * (B.const(ops[2][1]) & m)
        else:
            r = read(B, st, d) * read(B, st, ops[1])
        write(B, st, d, r & m)
    elif mn in ("shl", "shr", "sar"):
        a = read(B, st, d)
        c = shift_count(B, st, ops[1], w)
        if mn == "shl":
            r = B.shl(a, c)
        elif mn == "shr":
            r = B.lshr(a, c)
        else:
            r = B.ashr64(sext(B, a, w), c)
        write(B, st, d, r & m)
    elif mn in ("rol", "ror"):
        if ops[1][0] != "i":
            raise ValueError("rotate by register")
        write(B, st, d, rotate(B, read(B, st, d), ops[1][1] & (63 if w == 64 else 31), w, mn == "rol"))
    elif mn == "neg":
        write(B, st, d, (B.const(0) - read(B, st, d)) & m)
    elif mn == "not":
        write(B, st, d, (read(B, st, d) ^ m) & m)
    elif mn == "inc":
        write(B, st, d, (read(B, st, d) + B.const(1)) & m)
    elif mn == "dec":
        write(B, st, d, (read(B, st, d) - B.const(1)) & m)
    else:
        raise ValueError("unsupported " + mn)


def run(B, st, insns):
    st = list(st)
    for i in insns:
        step(B, st, i)
    return st


def written(insns):
    out = set()
    for mn, ops in insns:
        if ops and ops[0][0] == "r":
            out.add(ops[0][1])
    return out
