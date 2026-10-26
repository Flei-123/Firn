import random

import rawdq
from jxlbits import BitWriter

GRID = 32
COVER = {21: (16, 16), 22: (8, 16), 23: (16, 8), 24: (32, 32), 25: (16, 32), 26: (32, 16)}
NZ_SYMBOLS = [0, 1, 2, 3]
SPLIT_EXPONENT = 10

LAYOUTS = {
    "a24": [(0, 0, 24)],
    "b21": [(0, 0, 21), (16, 0, 21), (0, 16, 21), (16, 16, 21)],
    "c22": [(x, y, 22) for y in (0, 16) for x in (0, 8, 16, 24)],
    "d23": [(x, y, 23) for y in (0, 8, 16, 24) for x in (0, 16)],
    "e25": [(0, 0, 25), (16, 0, 25)],
    "f26": [(0, 0, 26), (0, 16, 26)],
    "g2122": [(0, 0, 21), (16, 0, 22), (24, 0, 22), (0, 16, 21), (16, 16, 21)],
    "h2523": [(0, 0, 25), (16, 0, 23), (16, 8, 23), (16, 16, 23), (16, 24, 23)],
    "i2622": [(0, 0, 26), (0, 16, 22), (8, 16, 22), (16, 16, 22), (24, 16, 22)],
}


def put_uint_config(bw, split_exponent):
    bw.put(split_exponent, 4)
    bw.put(0, 4)
    bw.put(0, 4)


def put_prefix_code(bw, symbols):
    alphabet = max(symbols) + 1
    rawdq.put_var_len_u16(bw, alphabet - 1)
    rawdq.put_simple_code(bw, alphabet, symbols)
    return rawdq.code_bits(symbols)


def check_layout(layout):
    covered = set()
    for x, y, t in layout:
        cx, cy = COVER[t]
        assert x + cx <= GRID and y + cy <= GRID
        for yy in range(y, y + cy):
            for xx in range(x, x + cx):
                assert (xx, yy) not in covered
                covered.add((xx, yy))
    assert len(covered) == GRID * GRID
    assert [(y, x) for x, y, _ in layout] == sorted((y, x) for x, y, _ in layout)


def ac_meta_bits(layout, qf_raw):
    check_layout(layout)
    types = [t for _, _, t in layout]
    tokens_by_channel = [
        [0] * 16,
        [0] * 16,
        [rawdq.pack_signed(t) for t in types] + [rawdq.pack_signed(qf_raw)] * len(types),
        [0] * (GRID * GRID),
    ]
    symbols = sorted(set(t for ch in tokens_by_channel for t in ch))
    assert len(symbols) <= 4, symbols
    while len(symbols) < 4:
        symbols.append(max(symbols) + 1 + len(symbols))
    symbols = sorted(set(symbols))
    bw = BitWriter()
    bw.put(len(layout) - 1, 10)
    bw.put(0, 1)
    bw.put(1, 1)
    bw.put(0, 2)
    rawdq.put_tree(bw, rawdq.PRED_ZERO)
    bw.put(0, 1)
    bw.put(1, 1)
    put_uint_config(bw, SPLIT_EXPONENT)
    table = put_prefix_code(bw, symbols)
    for ch in tokens_by_channel:
        for t in ch:
            rawdq.put_code(bw, table, t)
    return bw.bits


def block_coefficients(rng, cover, max_count):
    size = cover * 64
    count = rng.randint(1, max_count)
    positions = {}
    while len(positions) < count:
        r = rng.random() ** rng.choice((1, 2, 3))
        k = cover + int((size - cover - 1) * r)
        positions[k] = rng.choice((1, 1, 2, 3))
    return positions


def ac_group_bits(layout, seed, num_contexts, max_count=3):
    rng = random.Random(seed)
    bw = BitWriter()
    table = rawdq.code_bits(NZ_SYMBOLS)
    for x, y, t in layout:
        cx, cy = COVER[t]
        cover = cx * cy
        for c in (1, 0, 2):
            coefs = block_coefficients(rng, cover, max_count)
            rawdq.put_code(bw, table, len(coefs))
            last = max(coefs)
            for k in range(cover, last + 1):
                rawdq.put_code(bw, table, coefs.get(k, 0))
    return bw.bits


def hf_global_bits(dequant_bits, num_contexts):
    bw = BitWriter()
    bw.bits.extend(dequant_bits)
    bw.put(2, 2)
    bw.put(0, 1)
    bw.put(1, 1)
    bw.put(0, 2)
    bw.put(1, 1)
    rawdq.put_uint_config(bw)
    put_prefix_code(bw, NZ_SYMBOLS)
    return bw.bits
