import random

from jxlbits import BitWriter

KIND_W = [8, 8, 8, 8, 16, 32, 8, 8, 16, 8, 8, 64, 32, 128, 64, 256, 128]
KIND_H = [8, 8, 8, 8, 16, 32, 16, 32, 32, 8, 8, 64, 64, 128, 128, 256, 256]
PRED_ZERO = 0
PRED_LEFT = 1
PRED_GRADIENT = 5
ZERO_VALUES = [1, 2, 3, 4]
DELTAS = [0, -1, 1, -2]
MAX_VALUE = 24


def put_var_len_u16(bw, n):
    if n == 0:
        bw.put(0, 1)
        return
    bw.put(1, 1)
    nbits = n.bit_length() - 1
    bw.put(nbits, 4)
    if nbits:
        bw.put(n - (1 << nbits), nbits)


def put_uint_config(bw):
    bw.put(4, 4)
    bw.put(0, 3)
    bw.put(0, 3)


def put_simple_code(bw, alphabet, symbols):
    max_bits = (alphabet - 1).bit_length()
    bw.put(1, 2)
    bw.put(len(symbols) - 1, 2)
    for s in symbols:
        bw.put(s, max_bits)
    if len(symbols) == 4:
        bw.put(0, 1)


def code_bits(symbols):
    width = (len(symbols) - 1).bit_length()
    return {s: (i, width) for i, s in enumerate(sorted(symbols))}


def put_code(bw, table, symbol):
    value, width = table[symbol]
    for i in range(width - 1, -1, -1):
        bw.put((value >> i) & 1, 1)


def put_tree(bw, predictor):
    bw.put(0, 1)
    bw.put(1, 1)
    bw.put(0, 2)
    bw.put(1, 1)
    put_uint_config(bw)
    if predictor == PRED_ZERO:
        put_var_len_u16(bw, 0)
    else:
        put_var_len_u16(bw, predictor)
        put_simple_code(bw, predictor + 1, [0, predictor])
        table = code_bits([0, predictor])
        for token in (0, predictor, 0, 0, 0):
            put_code(bw, table, token)


def pack_signed(v):
    return 2 * v if v >= 0 else -2 * v - 1


def predict(rows, x, y, predictor):
    if predictor == PRED_ZERO:
        return 0
    left = rows[y][x - 1] if x > 0 else (rows[y - 1][x] if y > 0 else 0)
    if predictor == PRED_LEFT:
        return left
    top = rows[y - 1][x] if y > 0 else left
    topleft = rows[y - 1][x - 1] if x > 0 and y > 0 else left
    grad = left + top - topleft
    lo, hi = min(left, top), max(left, top)
    return max(lo, min(hi, grad))


def plane_tokens(rng, w, h, predictor):
    rows = [[0] * w for _ in range(h)]
    tokens = []
    for y in range(h):
        for x in range(w):
            if predictor == PRED_ZERO:
                v = rng.choice(ZERO_VALUES)
                tokens.append(pack_signed(v))
            else:
                pred = predict(rows, x, y, predictor)
                choices = [d for d in DELTAS if 1 <= pred + d <= MAX_VALUE]
                d = rng.choice(choices)
                v = pred + d
                tokens.append(pack_signed(d))
            rows[y][x] = v
    return tokens


def put_raw_table(bw, kind, predictor, den, rng):
    w, h = KIND_W[kind], KIND_H[kind]
    bw.put_f16(den)
    bw.put(0, 1)
    bw.put(1, 1)
    bw.put(0, 2)
    put_tree(bw, predictor)
    planes = [plane_tokens(rng, w, h, predictor) for _ in range(3)]
    symbols = sorted(set(ZERO_VALUES_TOKENS if predictor == PRED_ZERO else DELTA_TOKENS))
    bw.put(0, 1)
    bw.put(1, 1)
    put_uint_config(bw)
    put_var_len_u16(bw, symbols[-1])
    put_simple_code(bw, symbols[-1] + 1, symbols)
    table = code_bits(symbols)
    for plane in planes:
        for t in plane:
            put_code(bw, table, t)


ZERO_VALUES_TOKENS = [pack_signed(v) for v in ZERO_VALUES]
DELTA_TOKENS = [pack_signed(d) for d in DELTAS]

VARIANTS = {
    "raw1": (PRED_ZERO, [0]),
    "rawz": (PRED_ZERO, list(range(17))),
    "rawl": (PRED_LEFT, list(range(17))),
    "rawg": (PRED_GRADIENT, list(range(17))),
    "rawsm": (PRED_GRADIENT, [0, 1, 2, 3, 9, 10]),
}
DEN = 1.0 / 256.0


def variant_bits(name, den=DEN):
    predictor, kinds = VARIANTS[name]
    rng = random.Random(name)
    bw = BitWriter()
    bw.put(0, 1)
    for kind in range(17):
        if kind in kinds:
            bw.put(7, 3)
            put_raw_table(bw, kind, predictor, den, rng)
        else:
            bw.put(0, 3)
    return bw.bits
