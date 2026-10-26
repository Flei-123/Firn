import struct


class BitWriter:
    def __init__(self):
        self.bits = []

    def put(self, value, count):
        for i in range(count):
            self.bits.append((value >> i) & 1)

    def put_f16(self, value):
        self.put(struct.unpack("<H", struct.pack("<e", value))[0], 16)


def to_bytes(bits):
    out = bytearray((len(bits) + 7) // 8)
    for i, b in enumerate(bits):
        if b:
            out[i >> 3] |= 1 << (i & 7)
    return bytes(out)


def to_bits(data):
    return [(data[i >> 3] >> (i & 7)) & 1 for i in range(len(data) * 8)]


def encode_toc(sizes):
    bw = BitWriter()
    for s in sizes:
        if s < 1024:
            bw.put(0, 2)
            bw.put(s, 10)
        elif s < 17408:
            bw.put(1, 2)
            bw.put(s - 1024, 14)
        elif s < 4211712:
            bw.put(2, 2)
            bw.put(s - 17408, 22)
        else:
            bw.put(3, 2)
            bw.put(s - 4211712, 30)
    return to_bytes(bw.bits)
